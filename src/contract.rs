use cosmwasm_std::{attr, entry_point, to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult, Uint128, Uint64, WasmMsg, StdError, SubMsg, CosmosMsg, ReplyOn, WasmQuery, Reply, ContractResult, SubcallResponse, CanonicalAddr};

use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::state::{Config, PollStatus, State, CONFIG, POLL, POLL_VOTE, STATE, Proposal, PollInfoState, Migration};
use crate::helpers::user_total_weight;
use std::ops::Add;
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg};
use crate::error::ContractError;

const MAX_DESCRIPTION_LEN: u64 = 255;
const MIN_DESCRIPTION_LEN: u64 = 6;
const HOLDERS_MAX_REWARD: u8 = 20;
const WORKER_MAX_REWARD: u8 = 10;
// Note, you can use StdResult in some functions where you do not
// make use of the custom errors
#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    let config = Config {
        admin: deps.api.addr_canonicalize(info.sender.as_str()).unwrap(),
        poll_default_end_height: msg.poll_default_end_height,
        staking_contract_address: deps
            .api
            .addr_canonicalize(msg.staking_contract_address.as_str())
            .unwrap(),
        cw20_contract_address: deps
            .api
            .addr_canonicalize(msg.cw20_contract_address.as_str())
            .unwrap(),
    };
    CONFIG.save(deps.storage, &config)?;

    let state = State{
        required_collateral: msg.required_amount,
        denom: msg.denom,
        poll_id: 0,
        loterry_address: None
    };
    STATE.save(deps.storage, &state)?;

    let wasm_msg = WasmMsg::Instantiate {
        admin: Some(info.sender.to_string()),
        code_id: msg.code_id,
        msg: msg.message,
        send: vec![],
        label: msg.label,
    };
    let sub_message = SubMsg{
        id: 0,
        msg: CosmosMsg::Wasm(wasm_msg),
        gas_limit: None,
        reply_on: ReplyOn::Success
    };

    Ok(Response {
        submessages: vec![sub_message],
        messages: vec![],
        attributes: vec![
            attr("action", "instantiate"),
            attr("admin", info.sender),
            attr("code_id", msg.code_id),
        ],
        data: None,
    })
}

// And declare a custom Error variant for the ones where you will want to make use of it
#[entry_point]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> StdResult<Response> {
    match msg {
        ExecuteMsg::Vote { poll_id, approve } => try_vote(deps, info, env, poll_id, approve),
        ExecuteMsg::Poll {description, proposal, prizes_per_ranks, amount, recipient, migration} => try_create_poll(deps, info, env, description, proposal, prizes_per_ranks, amount, recipient, migration),
        _ => Ok(Response {
            submessages: vec![],
            messages: vec![],
            attributes: vec![],
            data: None,
        }),
    }
}
pub fn try_create_poll(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    description: String,
    proposal: Proposal,
    prizes_per_ranks: Option<Vec<u8>>,
    amount: Option<Uint128>,
    recipient: Option<String>,
    migration: Option<Migration>
) -> StdResult<Response> {
    let mut state = STATE.load(deps.storage)?;
    let config = CONFIG.load(deps.storage)?;
    // Increment and get the new poll id for bucket key
    let poll_id = state.poll_id.checked_add(1).unwrap();
    // Set the new counter
    state.poll_id = poll_id;

    // Check if some funds are sent
    let sent = match info.funds.len() {
        0 => Err(StdError::generic_err(format!(
            "you need to send {} as collateral in order to create a proposal",
            &state.required_collateral
        ))),
        1 => {
            if info.funds[0].denom == state.denom {
                Ok(info.funds[0].amount)
            } else {
                Err(StdError::generic_err(format!(
                    "you need to send {} as collateral in order to create a proposal",
                    &state.required_collateral
                )))
            }
        }
        _ => Err(StdError::generic_err(format!(
            "Only send {0} as collateral in order to create a proposal",
            &state.required_collateral
        ))),
    }?;

    // Handle the description is respecting length
    if (description.len() as u64) < MIN_DESCRIPTION_LEN {
        return Err(StdError::generic_err(format!(
            "Description min length {}",
            MIN_DESCRIPTION_LEN.to_string()
        )));
    } else if (description.len() as u64) > MAX_DESCRIPTION_LEN {
        return Err(StdError::generic_err(format!(
            "Description max length {}",
            MAX_DESCRIPTION_LEN.to_string()
        )));
    }

    let mut proposal_amount: Uint128 = Uint128::zero();
    let mut proposal_prize_rank: Vec<u8> = vec![];
    let mut proposal_human_address: Option<String> = None;
    let mut migration_to: Option<Migration> = None;

    let proposal_type = if let Proposal::HolderFeePercentage = proposal {
        match amount {
            Some(percentage) => {
                if percentage.u128() as u8 > HOLDERS_MAX_REWARD {
                    return Err(StdError::generic_err(format!(
                        "Amount between 0 to {}",
                        HOLDERS_MAX_REWARD
                    )));
                }
                proposal_amount = percentage;
            }
            None => {
                return Err(StdError::generic_err("Amount is required".to_string()));
            }
        }

        Proposal::HolderFeePercentage
    } else if let Proposal::DrandWorkerFeePercentage = proposal {
        match amount {
            Some(percentage) => {
                if percentage.u128() as u8 > WORKER_MAX_REWARD {
                    return Err(StdError::generic_err(format!(
                        "Amount between 0 to {}",
                        WORKER_MAX_REWARD
                    )));
                }
                proposal_amount = percentage;
            }
            None => {
                return Err(StdError::generic_err("Amount is required".to_string()));
            }
        }

        Proposal::DrandWorkerFeePercentage
    } else if let Proposal::JackpotRewardPercentage = proposal {
        match amount {
            Some(percentage) => {
                if percentage.u128() as u8 > 100 {
                    return Err(StdError::generic_err("Amount between 0 to 100".to_string()));
                }
                proposal_amount = percentage;
            }
            None => {
                return Err(StdError::generic_err("Amount is required".to_string()));
            }
        }

        Proposal::JackpotRewardPercentage
    } else if let Proposal::LotteryEveryBlockTime = proposal {
        match amount {
            Some(block_time) => {
                proposal_amount = block_time;
            }
            None => {
                return Err(StdError::generic_err(
                    "Amount block time required".to_string(),
                ));
            }
        }

        Proposal::LotteryEveryBlockTime
    } else if let Proposal::PrizePerRank = proposal {
        match prizes_per_ranks {
            Some(ranks) => {
                if ranks.len() != 4 {
                    return Err(StdError::generic_err(
                        "Ranks need to be in this format [0, 90, 10, 0] numbers between 0 to 100"
                            .to_string(),
                    ));
                }
                let mut total_percentage = 0;
                for rank in ranks.clone() {
                    if (rank as u8) > 100 {
                        return Err(StdError::generic_err(
                            "Numbers between 0 to 100".to_string(),
                        ));
                    }
                    total_percentage += rank;
                }
                // Ensure the repartition sum is 100%
                if total_percentage != 100 {
                    return Err(StdError::generic_err(
                        "Numbers total sum need to be equal to 100".to_string(),
                    ));
                }

                proposal_prize_rank = ranks;
            }
            None => {
                return Err(StdError::generic_err("Rank is required".to_string()));
            }
        }
        Proposal::PrizePerRank
    } else if let Proposal::AmountToRegister = proposal {
        match amount {
            Some(amount_to_register) => {
                proposal_amount = amount_to_register;
            }
            None => {
                return Err(StdError::generic_err("Amount is required".to_string()));
            }
        }
        Proposal::AmountToRegister
    } else if let Proposal::SecurityMigration = proposal {
        match migration {
            /*
                No need anymore migration address contract_addr, new_code_id, msg...
            */
            Some(migration) => {
                migration_to = Some(migration);
            }
            None => {
                return Err(StdError::generic_err(
                    "Migration is required".to_string(),
                ));
            }
        }
        Proposal::SecurityMigration
    } else if let Proposal::DaoFunding = proposal {
        match amount {
            Some(amount) => {

                if amount.is_zero() {
                    return Err(StdError::generic_err("Amount be higher than 0".to_string()));
                }

                // Get the contract balance prepare the tx
                let msg_balance = Cw20QueryMsg::Balance {
                    address: env.contract.address.to_string(),
                };
                let loterra_human = deps
                    .api
                    .addr_humanize(&config.cw20_contract_address)?;

                let res_balance = WasmQuery::Smart {
                    contract_addr: loterra_human.to_string(),
                    msg: to_binary(&msg_balance)?,
                };
                let loterra_balance: BalanceResponse = deps.querier.query(&res_balance.into())?;

                if loterra_balance.balance.is_zero() {
                    return Err(StdError::generic_err(
                        "No more funds to fund project".to_string(),
                    ));
                }
                if loterra_balance.balance.u128() < amount.u128() {
                    return Err(StdError::generic_err(format!(
                        "You need {} we only can fund you up to {}",
                        amount, loterra_balance.balance
                    )));
                }

                proposal_amount = amount;
                proposal_human_address = recipient;
            }
            None => {
                return Err(StdError::generic_err("Amount required".to_string()));
            }
        }
        Proposal::DaoFunding
    } else if let Proposal::StakingContractMigration = proposal {
        match migration {
            Some(migration) => {
                migration_to = Some(migration);
            }
            None => {
                return Err(StdError::generic_err(
                    "Migration address is required".to_string(),
                ));
            }
        }
        Proposal::StakingContractMigration
    } else if let Proposal::PollSurvey = proposal {
        Proposal::PollSurvey
    } else {
        return Err(StdError::generic_err(
            "Proposal type not founds".to_string(),
        ));
    };

    let sender_to_canonical = deps.api.addr_canonicalize(&info.sender.as_str())?;

    let new_poll = PollInfoState {
        creator: sender_to_canonical,
        status: PollStatus::InProgress,
        end_height: env.block.height + config.poll_default_end_height,
        start_height: env.block.height,
        description,
        weight_yes_vote: Uint128::zero(),
        weight_no_vote: Uint128::zero(),
        yes_vote: 0,
        no_vote: 0,
        amount: proposal_amount,
        prizes_per_ranks: proposal_prize_rank,
        proposal: proposal_type,
        recipient: proposal_human_address,
        migration: migration_to
    };

    // Save poll
    POLL.save(deps.storage, &state.poll_id.to_be_bytes(), &new_poll)?;

    // Save state
    STATE.save(deps.storage, &state)?;

    Ok(Response {
        submessages: vec![],
        messages: vec![],
        data: None,
        attributes: vec![
            attr("action", "create poll"),
            attr("poll_id", poll_id),
            attr("poll_creation_result", "success"),
        ],
    })
}
pub fn try_vote(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    poll_id: u64,
    approve: bool,
) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;

    let mut poll = POLL.load(deps.storage, &poll_id.to_be_bytes())?;
    // Ensure the sender not sending funds accidentally
    if !info.funds.is_empty() {
        return Err(StdError::generic_err("do not send funds"));
    }
    let sender = deps.api.addr_canonicalize(info.sender.as_ref())?;

    // Ensure the poll is still valid
    if env.block.height > poll.end_height {
        return Err(StdError::generic_err("poll expired"));
    }
    // Ensure the poll is still valid
    if poll.status != PollStatus::InProgress {
        return Err(StdError::generic_err("poll closed"));
    }

    POLL_VOTE.update(
        deps.storage,
        (&poll_id.to_be_bytes(), &sender),
        |exist| match exist {
            None => Ok(approve),
            Some(_) => Err(StdError::generic_err("already voted")),
        },
    )?;

    // Get the sender weight
    let weight = user_total_weight(&deps, &config, &info.sender);
    //let weight = Uint128(200);
    // Only stakers can vote
    if weight.is_zero() {
        return Err(StdError::generic_err("only stakers can vote"));
    }

    // save weight
    let voice = 1;
    if approve {
        poll.yes_vote += voice;
        poll.weight_yes_vote = poll.weight_yes_vote.add(weight);
    } else {
        poll.no_vote += voice;
        poll.weight_no_vote = poll.weight_no_vote.add(weight);
    }
    // overwrite poll info
    POLL.save(deps.storage, &poll_id.to_be_bytes(), &poll)?;

    Ok(Response {
        submessages: vec![],
        messages: vec![],
        attributes: vec![
            attr("action", "vote"),
            attr("proposal_id", poll_id.to_string()),
            attr("voting_result", "success"),
        ],
        data: None,
    })
}
/*
pub fn try_increment(deps: DepsMut) -> Result<Response, ContractError> {
    STATE.update(deps.storage, |mut state| -> Result<_, ContractError> {
        state.count += 1;
        Ok(state)
    })?;

    Ok(Response::default())
}

pub fn try_reset(deps: DepsMut, info: MessageInfo, count: i32) -> Result<Response, ContractError> {
    STATE.update(deps.storage, |mut state| -> Result<_, ContractError> {
        if info.sender != state.owner {
            return Err(ContractError::Unauthorized {});
        }
        state.count = count;
        Ok(state)
    })?;
    Ok(Response::default())
} */

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> Result<Response, ContractError> {

    match msg.id {
        0 => loterra_instance_reply(deps, env, msg.result),
        _ => Err(ContractError::Unauthorized {}),
    }
}

pub fn loterra_instance_reply(
    deps: DepsMut,
    _env: Env,
    msg: ContractResult<SubcallResponse>,
) -> Result<Response, ContractError> {
    let mut state = STATE.load(deps.storage)?;
    /*
        TODO: Save the address of LoTerra contract lottery to the state
     */
    match msg {
        ContractResult::Ok(subcall) => {
            let contract_address = subcall
                .events
                .into_iter()
                .find(|e| e.kind == "instantiate_contract")
                .and_then(|ev| {
                    ev.attributes
                        .into_iter()
                        .find(|attr| attr.key == "contract_address")
                        .and_then(|addr| Some(addr.value))
                })
                .unwrap();
            state.loterry_address = Some(deps.api.addr_canonicalize(&contract_address.as_str())?);
            STATE.save(deps.storage, &state)?;

            Ok(Response {
                submessages: vec![],
                messages: vec![],
                attributes: vec![
                    attr("lottery-address", contract_address),
                    attr("lottery-instantiate", "success"),
                ],
                data: None,
            })
        }
        ContractResult::Err(_) => Err(ContractError::Unauthorized {}),
    }
}


#[entry_point]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetPoll { .. } => to_binary(&query_count(deps)?),
    }
}

fn query_count(deps: Deps) -> StdResult<u64> {
    let state = STATE.load(deps.storage)?;
    Ok(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{coins, from_binary};
    use crate::mock_querier::{mock_dependencies_custom};

    struct BeforeAll {
        default_sender: String,
        default_sender_two: String,
        default_sender_owner: String,
    }
    fn before_all() -> BeforeAll {
        BeforeAll {
            default_sender: "addr0000".to_string(),
            default_sender_two: "addr0001".to_string(),
            default_sender_owner: "addr0002".to_string(),
        }
    }

    fn default_init(deps: DepsMut) {
        let msg = InstantiateMsg {
            code_id: 0,
            message: Default::default(),
            label: "".to_string(),
            staking_contract_address: "staking".to_string(),
            cw20_contract_address: "cw20".to_string(),
            poll_default_end_height: 0,
            required_amount: Uint128(100_000_000),
            denom: "uusd".to_string()
        };
        let info = mock_info("creator", &coins(1000, "earth"));
        // we can just call .unwrap() to assert this was a success
        let res = instantiate(deps, mock_env(), info, msg).unwrap();
    }
    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies(&[]);

        let msg = InstantiateMsg {
            code_id: 0,
            message: Default::default(),
            label: "Hello world contract".to_string(),
            staking_contract_address: "staking".to_string(),
            cw20_contract_address: "cw20".to_string(),
            poll_default_end_height: 0,
            required_amount: Uint128(100_000_000),
            denom: "uusd".to_string()
        };
        let info = mock_info("creator", &coins(1000, "earth"));

        // we can just call .unwrap() to assert this was a success
        let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(0, res.messages.len());
    }

    mod vote {
        use super::*;
        use cosmwasm_std::{Api, Decimal, Coin, Event};
        use crate::state::{PollInfoState, Proposal};

        // handle_vote
        fn create_poll(deps: DepsMut) {
            let msg = ExecuteMsg::Poll {
                description: "This is my first proposal".to_string(),
                proposal: Proposal::LotteryEveryBlockTime,
                amount: Option::from(Uint128(22)),
                prizes_per_ranks: None,
                recipient: None,
                migration: None
            };

            let _res = execute(deps, mock_env(), mock_info("addr0000", &[Coin{ denom: "uusd".to_string(), amount: Uint128(100) }]), msg).unwrap();
            println!("{:?}", _res);
        }
        #[test]
        fn do_not_send_funds() {
            let before_all = before_all();
            let mut deps = mock_dependencies(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128(9_000_000),
            }]);
            default_init(deps.as_mut());
            create_poll(deps.as_mut());

            let env = mock_env();
            let info = mock_info(
                before_all.default_sender.as_str().clone(),
                &[Coin {
                    denom: "ust".to_string(),
                    amount: Uint128(9_000_000),
                }],
            );
            let msg = ExecuteMsg::Vote {
                poll_id: 1,
                approve: false,
            };
            let res = execute(deps.as_mut(), env, info, msg);
            println!("{:?}", res);
            match res {
                Err(StdError::GenericErr { msg, .. }) => {
                    assert_eq!(msg, "do not send funds")
                }
                _ => panic!("Unexpected error"),
            }
        }
        #[test]
        fn poll_deactivated() {
            let before_all = before_all();
            let mut deps = mock_dependencies(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128(9_000_000),
            }]);
            default_init(deps.as_mut());
            create_poll(deps.as_mut());

            // Save to storage
            POLL.update(
                deps.as_mut().storage,
                &1u64.to_be_bytes(),
                |poll| -> StdResult<PollInfoState> {
                    match poll {
                        None => Err(StdError::generic_err("error")),
                        Some(poll_state) => {
                            let mut poll_data = poll_state;
                            // Update the status to passed
                            poll_data.status = PollStatus::RejectedByCreator;
                            Ok(poll_data)
                        }
                    }
                },
            )
                .unwrap();

            let env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let msg = ExecuteMsg::Vote {
                poll_id: 1,
                approve: false,
            };
            let res = execute(deps.as_mut(), env, info, msg);
            println!("{:?}", res);
            match res {
                Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "poll closed"),
                _ => panic!("Unexpected error"),
            }
        }
        #[test]
        fn poll_expired() {
            let before_all = before_all();
            let mut deps = mock_dependencies(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128(9_000_000),
            }]);
            default_init(deps.as_mut());
            create_poll(deps.as_mut());

            let mut env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let poll_state = POLL
                .load(deps.as_ref().storage, &1_u64.to_be_bytes())
                .unwrap();
            env.block.height = poll_state.end_height + 1;

            let msg = ExecuteMsg::Vote {
                poll_id: 1,
                approve: false,
            };
            let res = execute(deps.as_mut(), env, info, msg);
            println!("{:?}", res);
            match res {
                Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "poll expired"),
                _ => panic!("Unexpected error"),
            }
        }
        #[test]
        fn only_stakers_with_bonded_tokens_can_vote() {
            let before_all = before_all();
            let mut deps = mock_dependencies_custom(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128(9_000_000),
            }]);
            deps.querier.with_holder(
                before_all.default_sender.clone(),
                Uint128(0),
                Decimal::zero(),
                Decimal::zero(),
            );

            default_init(deps.as_mut());
            create_poll(deps.as_mut());

            let env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let msg = ExecuteMsg::Vote {
                poll_id: 1,
                approve: false,
            };
            let res = execute(deps.as_mut(), env, info, msg);
            match res {
                Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "only stakers can vote"),
                _ => panic!("Unexpected error"),
            }
        }
        #[test]
        fn success() {
            let before_all = before_all();
            let mut deps = mock_dependencies_custom(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128(9_000_000),
            }]);
            deps.querier.with_holder(
                before_all.default_sender.clone(),
                Uint128(150_000),
                Decimal::zero(),
                Decimal::zero(),
            );
            default_init(deps.as_mut());
            create_poll(deps.as_mut());

            let env = mock_env();
            let info = mock_info(before_all.default_sender.as_str(), &[]);
            let poll_id: u64 = 1;
            let approve = false;
            let msg = ExecuteMsg::Vote { poll_id, approve };
            let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone()).unwrap();
            let poll_state = POLL
                .load(deps.as_ref().storage, &poll_id.to_be_bytes())
                .unwrap();
            assert_eq!(res.attributes.len(), 3);
            assert_eq!(poll_state.no_vote, 1);
            assert_eq!(poll_state.yes_vote, 0);
            assert_eq!(poll_state.weight_yes_vote, Uint128::zero());
            assert_eq!(poll_state.weight_no_vote, Uint128(150_000));

            let sender_to_canonical = deps
                .api
                .addr_canonicalize(&before_all.default_sender)
                .unwrap();
            let vote_state = POLL_VOTE
                .load(
                    deps.as_ref().storage,
                    (
                        &poll_id.to_be_bytes().clone(),
                        sender_to_canonical.as_slice(),
                    ),
                )
                .unwrap();
            assert_eq!(vote_state, approve);

            // Try to vote multiple times
            let res = execute(deps.as_mut(), env, info, msg);
            match res {
                Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "already voted"),
                _ => panic!("Unexpected error"),
            }
        }
    }
/*
    fn default_init(deps: DepsMut) {
        let msg = InstantiateMsg {
            code_id: 0,
            message: Default::default(),
            label: "".to_string(),
            staking_contract_address: "".to_string(),
            poll_default_end_height: 0,
        };
        let info = mock_info("creator", &coins(1000, "earth"));
        // we can just call .unwrap() to assert this was a success
        let res = instantiate(deps, mock_env(), info, msg).unwrap();
    }
    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies(&[]);

        let msg = InstantiateMsg {
            code_id: 0,
            message: Default::default(),
            label: "".to_string(),
            staking_contract_address: "".to_string(),
            poll_default_end_height: 0,
        };
        let info = mock_info("creator", &coins(1000, "earth"));

        // we can just call .unwrap() to assert this was a success
        let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(0, res.messages.len());

        // it worked, let's query the state
        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetPoll { poll_id: 0 }).unwrap();
        let value: u64 = from_binary(&res).unwrap();
        assert_eq!(1, value);
    }
    #[test]
    fn vote() {
        let mut deps = mock_dependencies(&[]);
        default_init(deps.as_mut());

        let info = mock_info("player", &coins(100, "earth"));
        let env = mock_env();
        let msg = ExecuteMsg::Vote {
            poll_id: 1,
            approve: false,
        };
        let res = execute(deps.as_mut(), env, info, msg);
        println!("{:?}", res)
    } */
    /*
    #[test]
    fn increment() {
        let mut deps = mock_dependencies(&coins(2, "token"));

        let msg = InstantiateMsg { count: 17 };
        let info = mock_info("creator", &coins(2, "token"));
        let _res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        // beneficiary can release it
        let info = mock_info("anyone", &coins(2, "token"));
        let msg = ExecuteMsg::Increment {};
        let _res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        // should increase counter by 1
        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetCount {}).unwrap();
        let value: CountResponse = from_binary(&res).unwrap();
        assert_eq!(18, value.count);
    }

    #[test]
    fn reset() {
        let mut deps = mock_dependencies(&coins(2, "token"));

        let msg = InstantiateMsg { count: 17 };
        let info = mock_info("creator", &coins(2, "token"));
        let _res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        // beneficiary can release it
        let unauth_info = mock_info("anyone", &coins(2, "token"));
        let msg = ExecuteMsg::Reset { count: 5 };
        let res = execute(deps.as_mut(), mock_env(), unauth_info, msg);
        match res {
            Err(ContractError::Unauthorized {}) => {}
            _ => panic!("Must return unauthorized error"),
        }

        // only the original creator can reset the counter
        let auth_info = mock_info("creator", &coins(2, "token"));
        let msg = ExecuteMsg::Reset { count: 5 };
        let _res = execute(deps.as_mut(), mock_env(), auth_info, msg).unwrap();

        // should now be 5
        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetCount {}).unwrap();
        let value: CountResponse = from_binary(&res).unwrap();
        assert_eq!(5, value.count);
    } */
}

//let result = ContractResult::Ok(SubcallResponse{ events: vec![Event{ kind: "instantiate_contract".to_string(), attributes: vec![attr("contract_address", "loterra")] }], data: None });
//let reply = reply(deps.as_mut(), env.clone(), Reply{ id: 0, result }).unwrap();
