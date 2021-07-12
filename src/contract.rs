use cosmwasm_std::{
    attr, entry_point, to_binary, BankMsg, Binary, Coin, ContractResult, CosmosMsg, Deps, DepsMut,
    Env, MessageInfo, Reply, ReplyOn, Response, StdError, StdResult, SubMsg, SubcallResponse,
    Uint128, WasmMsg, WasmQuery,
};

use crate::error::ContractError;
use crate::helpers::{reject_proposal, total_weight, user_total_weight};
use crate::msg::{
    ConfigResponse, ExecuteMsg, GetPollResponse, InstantiateMsg, LoterraLottery, LoterraStaking,
    QueryMsg, StakingStateResponse, StateResponse,
};
use crate::state::{
    Config, Migration, PollInfoState, PollStatus, Proposal, State, CONFIG, POLL, POLL_VOTE, STATE,
};
use crate::taxation::deduct_tax;
use cw20::{BalanceResponse, Cw20QueryMsg};
use std::ops::Add;

const MAX_DESCRIPTION_LEN: u64 = 255;
const MIN_DESCRIPTION_LEN: u64 = 6;
const HOLDERS_MAX_REWARD: u8 = 20;
const WORKER_MAX_REWARD: u8 = 10;
const YES_WEIGHT: u128 = 50;
const NO_WEIGHT: u128 = 33;
const QUORUM: u128 = 10;
// Note, you can use StdResult in some functions where you do not
// make use of the custom errors
#[cfg_attr(not(feature = "library"), entry_point)]
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

    let state = State {
        required_collateral: msg.required_amount,
        denom: msg.denom,
        poll_id: 0,
        loterry_address: None,
    };
    STATE.save(deps.storage, &state)?;

    let wasm_msg = WasmMsg::Instantiate {
        admin: Some(info.sender.to_string()),
        code_id: msg.code_id,
        msg: msg.message,
        send: vec![],
        label: msg.label,
    };
    let sub_message = SubMsg {
        id: 0,
        msg: CosmosMsg::Wasm(wasm_msg),
        gas_limit: None,
        reply_on: ReplyOn::Success,
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
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    match msg {
        ExecuteMsg::Vote { poll_id, approve } => try_vote(deps, info, env, poll_id, approve),
        ExecuteMsg::Poll {
            description,
            proposal,
            prizes_per_ranks,
            amount,
            recipient,
            migration,
            contract_address,
        } => try_create_poll(
            deps,
            info,
            env,
            description,
            proposal,
            prizes_per_ranks,
            amount,
            recipient,
            migration,
            contract_address,
        ),
        ExecuteMsg::PresentPoll { poll_id } => try_present(deps, info, env, poll_id),
        ExecuteMsg::RejectPoll { poll_id } => try_reject(deps, info, env, poll_id),
    }
}
#[allow(clippy::too_many_arguments)]
pub fn try_create_poll(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    description: String,
    proposal: Proposal,
    prizes_per_ranks: Option<Vec<u8>>,
    amount: Option<Uint128>,
    recipient: Option<String>,
    migration: Option<Migration>,
    contract_address: String,
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
    } else if let Proposal::PrizesPerRanks = proposal {
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
        Proposal::PrizesPerRanks
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
                return Err(StdError::generic_err("Migration is required".to_string()));
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
                let loterra_human = deps.api.addr_humanize(&config.cw20_contract_address)?;

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
        match recipient {
            Some(recipient) => {
                proposal_human_address = Some(recipient);
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
        migration: migration_to,
        collateral: sent,
        contract_address: deps.api.addr_canonicalize(&contract_address.as_str())?,
        applied: false,
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
fn try_reject(deps: DepsMut, info: MessageInfo, env: Env, poll_id: u64) -> StdResult<Response> {
    let state = STATE.load(deps.storage)?;
    let poll = POLL.load(deps.storage, &poll_id.to_be_bytes())?;
    let sender = deps.api.addr_canonicalize(&info.sender.as_str())?;

    // Ensure the sender not sending funds accidentally
    if !info.funds.is_empty() {
        return Err(StdError::generic_err(
            "Do not send funds with reject proposal",
        ));
    }
    // Ensure end proposal height is not expired
    if poll.end_height < env.block.height {
        return Err(StdError::generic_err("Proposal expired"));
    }
    // Ensure only the creator can reject a proposal OR the status of the proposal is still in progress
    if poll.creator != sender || poll.status != PollStatus::InProgress {
        return Err(StdError::generic_err("Unauthorized"));
    }

    POLL.update(deps.storage, &poll_id.to_be_bytes(), |poll| match poll {
        None => Err(StdError::generic_err("Poll not found")),
        Some(poll_info) => {
            let mut poll = poll_info;
            poll.status = PollStatus::RejectedByCreator;
            poll.end_height = env.block.height;
            Ok(poll)
        }
    })?;

    /*
       TODO: Build collateral wasm send
        Also we need to check what we can do with this collateral.
        Probably send it to the lottery contract???
    */
    let msg = BankMsg::Send {
        to_address: deps
            .api
            .addr_humanize(&state.loterry_address.unwrap())?
            .to_string(),
        amount: vec![deduct_tax(
            &deps.querier,
            Coin {
                denom: state.denom,
                amount: poll.collateral,
            },
        )?],
    };

    Ok(Response {
        submessages: vec![],
        messages: vec![msg.into()],
        data: None,
        attributes: vec![
            attr("action", "creator reject the proposal"),
            attr("poll_id", poll_id),
        ],
    })
}

fn try_present(deps: DepsMut, info: MessageInfo, env: Env, poll_id: u64) -> StdResult<Response> {
    // Load storage
    let state = STATE.load(deps.storage)?;
    let config = CONFIG.load(deps.storage)?;
    let poll = POLL.load(deps.storage, &poll_id.to_be_bytes())?;

    // Ensure the sender not sending funds accidentally
    if !info.funds.is_empty() {
        return Err(StdError::generic_err(
            "Do not send funds with present proposal",
        ));
    }
    // Ensure the proposal is still in Progress
    if poll.status != PollStatus::InProgress {
        return Err(StdError::generic_err("Unauthorized"));
    }
    let query_total_stake = LoterraStaking::State {};
    let query = WasmQuery::Smart {
        contract_addr: deps
            .api
            .addr_humanize(&config.staking_contract_address)?
            .to_string(),
        msg: to_binary(&query_total_stake)?,
    };
    let res_total_bonded: StakingStateResponse = deps.querier.query(&query.into())?;

    let total_weight_bonded = total_weight(&deps, &config);
    let total_vote_weight = poll.weight_yes_vote.add(poll.weight_no_vote);

    let total_yes_weight_percentage = if !poll.weight_yes_vote.is_zero() {
        poll.weight_yes_vote.u128() * 100 / total_vote_weight.u128()
    } else {
        0
    };
    let total_no_weight_percentage = if !poll.weight_no_vote.is_zero() {
        poll.weight_no_vote.u128() * 100 / total_vote_weight.u128()
    } else {
        0
    };

    if poll.weight_yes_vote.add(poll.weight_no_vote).u128() * 100 / total_weight_bonded.u128() < 50
    {
        // Ensure the proposal is ended
        if poll.end_height > env.block.height {
            return Err(StdError::generic_err("Proposal still in progress"));
        }
    }
    // Get the quorum min 10%
    let total_vote_weight_in_percentage = if !total_vote_weight.is_zero() {
        total_vote_weight.u128() * 100_u128 / res_total_bonded.total_balance.u128()
    } else {
        0_u128
    };

    // Reject the proposal
    // Based on the recommendation of security audit
    // We recommend to not reject votes based on the number of votes, but rather by the stake of the voters.
    if total_yes_weight_percentage < YES_WEIGHT
        || total_no_weight_percentage > NO_WEIGHT
        || total_vote_weight_in_percentage < QUORUM
    {
        return reject_proposal(deps, poll_id);
    }

    // Save to storage
    POLL.update(deps.storage, &poll_id.to_be_bytes(), |poll| match poll {
        None => Err(StdError::generic_err("Not found")),
        Some(poll_info) => {
            let mut poll = poll_info;
            poll.status = PollStatus::Passed;
            Ok(poll)
        }
    })?;

    STATE.save(deps.storage, &state)?;

    /*
       TODO: Build this test
        Also we need to check what we can do with this collateral.
        Probably send it to the lottery contract???
    */
    let mut msg = vec![];
    if poll.status == PollStatus::Passed {
        msg.push(
            BankMsg::Send {
                to_address: deps.api.addr_humanize(&poll.creator)?.to_string(),
                amount: vec![deduct_tax(
                    &deps.querier,
                    Coin {
                        denom: state.denom,
                        amount: poll.collateral,
                    },
                )?],
            }
            .into(),
        )
    } else {
        msg.push(
            BankMsg::Send {
                to_address: deps
                    .api
                    .addr_humanize(&state.loterry_address.unwrap())?
                    .to_string(),
                amount: vec![deduct_tax(
                    &deps.querier,
                    Coin {
                        denom: state.denom,
                        amount: poll.collateral,
                    },
                )?],
            }
            .into(),
        )
    }

    /*
       Create a Reply message in order to catch the result
    */
    // Call LoTerra lottery contract and get the response
    let sub_msg = LoterraLottery::PresentPoll { poll_id };
    let execute_sub_msg = WasmMsg::Execute {
        contract_addr: deps.api.addr_humanize(&poll.contract_address)?.to_string(),
        msg: to_binary(&sub_msg)?,
        send: vec![],
    };
    let sub = SubMsg {
        id: 1,
        msg: CosmosMsg::Wasm(execute_sub_msg),
        gas_limit: None,
        reply_on: ReplyOn::Always,
    };

    Ok(Response {
        submessages: vec![sub],
        messages: msg,
        data: None,
        attributes: vec![
            attr("action", "present poll"),
            attr("poll_id", poll_id),
            attr("poll_result", "approved"),
        ],
    })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.id {
        0 => loterra_instance_reply(deps, env, msg.result),
        1 => loterra_lottery_reply(deps, env, msg.result),
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
       Save the address of LoTerra contract lottery to the state
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
                        .map(|addr| addr.value)
                })
                .unwrap();
            state.loterry_address = Some(deps.api.addr_canonicalize(&contract_address.as_str())?);
            STATE.save(deps.storage, &state)?;

            let update = WasmMsg::UpdateAdmin {
                contract_addr: contract_address.clone(),
                admin: contract_address.clone(),
            };

            Ok(Response {
                submessages: vec![],
                messages: vec![update.into()],
                attributes: vec![
                    attr("lottery-address", contract_address.clone()),
                    attr("lottery-instantiate", "success"),
                    attr("lottery-update-admin", contract_address),
                ],
                data: None,
            })
        }
        ContractResult::Err(_) => Err(ContractError::Unauthorized {}),
    }
}

pub fn loterra_lottery_reply(
    deps: DepsMut,
    _env: Env,
    msg: ContractResult<SubcallResponse>,
) -> Result<Response, ContractError> {
    match msg {
        ContractResult::Ok(subcall) => {
            let (poll_result, poll_id) = subcall
                .events
                .into_iter()
                .find(|e| e.kind == "message")
                .and_then(|ev| {
                    let res = ev
                        .clone()
                        .attributes
                        .into_iter()
                        .find(|attr| attr.key == "applied")?;
                    let id = ev
                        .attributes
                        .into_iter()
                        .find(|attr| attr.key == "poll_id")?;

                    Some((res.value, id.value))
                })
                .unwrap();

            let id = poll_id.parse::<u64>().unwrap();
            POLL.update(deps.storage, &id.to_be_bytes(), |poll| match poll {
                None => Err(ContractError::Unauthorized {}),
                Some(poll_info) => {
                    let mut update_poll = poll_info;
                    update_poll.applied = poll_result.parse().unwrap();
                    Ok(update_poll)
                }
            })?;

            Ok(Response {
                submessages: vec![],
                messages: vec![],
                attributes: vec![attr("applied", poll_result), attr("poll_id", poll_id)],
                data: None,
            })
        }
        ContractResult::Err(_) => Err(ContractError::Unauthorized {}),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    let response = match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps)?)?,
        QueryMsg::State {} => to_binary(&query_state(deps)?)?,
        QueryMsg::GetPoll { poll_id } => to_binary(&query_poll(deps, poll_id)?)?,
    };
    Ok(response)
}
fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config = CONFIG.load(deps.storage)?;
    Ok(config)
}
fn query_state(deps: Deps) -> StdResult<StateResponse> {
    let state = STATE.load(deps.storage)?;
    Ok(state)
}
fn query_poll(deps: Deps, poll_id: u64) -> StdResult<GetPollResponse> {
    let poll = match POLL.may_load(deps.storage, &poll_id.to_be_bytes())? {
        Some(poll) => Some(poll),
        None => {
            return Err(StdError::generic_err("Not found"));
        }
    }
    .unwrap();

    Ok(GetPollResponse {
        creator: deps.api.addr_humanize(&poll.creator)?,
        status: poll.status,
        end_height: poll.end_height,
        start_height: poll.start_height,
        description: poll.description,
        amount: poll.amount,
        prizes_per_ranks: poll.prizes_per_ranks,
        weight_yes_vote: poll.weight_yes_vote,
        weight_no_vote: poll.weight_no_vote,
        yes_vote: poll.yes_vote,
        no_vote: poll.no_vote,
        proposal: poll.proposal,
        migration: poll.migration,
        recipient: poll.recipient,
        collateral: poll.collateral,
        contract_address: deps.api.addr_humanize(&poll.contract_address)?,
        applied: poll.applied,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock_querier::mock_dependencies_custom;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{coins, from_binary};

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
            denom: "uusd".to_string(),
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
            denom: "uusd".to_string(),
        };
        let info = mock_info("creator", &coins(1000, "earth"));

        // we can just call .unwrap() to assert this was a success
        let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(0, res.messages.len());
    }

    mod proposal {
        use super::*;
        use cosmwasm_std::{Api, Coin};

        // handle_proposal
        #[test]
        fn description_min_error() {
            let before_all = before_all();
            let mut deps = mock_dependencies(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128(9_000_000),
            }]);
            default_init(deps.as_mut());
            let env = mock_env();
            let msg = ExecuteMsg::Poll {
                description: "This".to_string(),
                proposal: Proposal::LotteryEveryBlockTime,
                amount: Option::from(Uint128(22)),
                recipient: None,
                prizes_per_ranks: None,
                migration: None,
                contract_address: "lottery".to_string(),
            };
            let info = mock_info(
                before_all.default_sender.as_str().clone(),
                &[Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128(100_000_000),
                }],
            );
            let res = execute(deps.as_mut(), env, info, msg);
            println!("{:?}", res);
            match res {
                Err(StdError::GenericErr { msg, .. }) => {
                    assert_eq!(msg, "Description min length 6")
                }
                _ => panic!("Unexpected error"),
            }
        }
        #[test]
        fn description_max_error() {
            let before_all = before_all();
            let mut deps = mock_dependencies(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128(9_000_000),
            }]);
            default_init(deps.as_mut());
            let env = mock_env();
            let msg = ExecuteMsg::Poll {
                description: "let env = mock_env(before_all.default_sender.clone(), &[]);\
                 let env = mock_env(before_all.default_sender.clone(), &[]); let env \
                 = mock_env(before_all.default_sender.clone(), &[]); let env = mock_env(before_all.default_sender.clone(), &[]);\
                 let env = mock_env(before_all.default_sender.clone(), &[]);let env = mock_env(before_all.default_sender.clone(), &[]);
                 ".to_string(),
                proposal: Proposal::LotteryEveryBlockTime,
                amount: Option::from(Uint128(22)),
                recipient: None,
                prizes_per_ranks: None,
                migration: None,
                contract_address: "lottery".to_string()
            };
            let info = mock_info(
                before_all.default_sender.as_str().clone(),
                &[Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128(100_000_000),
                }],
            );
            let res = execute(deps.as_mut(), env, info, msg);
            println!("{:?}", res);
            match res {
                Err(StdError::GenericErr { msg, .. }) => {
                    assert_eq!(msg, "Description max length 255")
                }
                _ => panic!("Unexpected error"),
            }
        }
        #[test]
        fn send_funds() {
            let before_all = before_all();
            let mut deps = mock_dependencies(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128(9_000_000),
            }]);
            default_init(deps.as_mut());
            let env = mock_env();
            let msg = ExecuteMsg::Poll {
                description: "This is my first proposal".to_string(),
                proposal: Proposal::LotteryEveryBlockTime,
                amount: Option::from(Uint128(22)),
                recipient: None,
                prizes_per_ranks: None,
                migration: None,
                contract_address: "lottery".to_string(),
            };
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let res = execute(deps.as_mut(), env, info, msg);
            println!("{:?}", res);
            match res {
                Err(StdError::GenericErr { msg, .. }) => {
                    assert_eq!(
                        msg,
                        "you need to send 100000000 as collateral in order to create a proposal"
                    )
                }
                _ => panic!("Unexpected error"),
            }
        }

        fn msg_constructor_none(proposal: Proposal) -> ExecuteMsg {
            ExecuteMsg::Poll {
                description: "This is my first proposal".to_string(),
                proposal,
                amount: None,
                recipient: None,
                prizes_per_ranks: None,
                migration: None,
                contract_address: "lottery".to_string(),
            }
        }
        fn msg_constructor_amount_out(proposal: Proposal) -> ExecuteMsg {
            ExecuteMsg::Poll {
                description: "This is my first proposal".to_string(),
                proposal,
                amount: Option::from(Uint128(250)),
                recipient: None,
                prizes_per_ranks: None,
                migration: None,
                contract_address: "lottery".to_string(),
            }
        }

        fn msg_constructor_prize_len_out(proposal: Proposal) -> ExecuteMsg {
            ExecuteMsg::Poll {
                description: "This is my first proposal".to_string(),
                proposal,
                amount: None,
                recipient: None,
                prizes_per_ranks: Option::from(vec![10, 20, 23, 23, 23, 23]),
                migration: None,
                contract_address: "lottery".to_string(),
            }
        }

        fn msg_constructor_prize_sum_out(proposal: Proposal) -> ExecuteMsg {
            ExecuteMsg::Poll {
                description: "This is my first proposal".to_string(),
                proposal,
                amount: None,
                recipient: None,
                prizes_per_ranks: Option::from(vec![100, 20, 23, 23]),
                migration: None,
                contract_address: "lottery".to_string(),
            }
        }

        #[test]
        fn all_proposal_amount_error() {
            let before_all = before_all();
            let mut deps = mock_dependencies(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128(9_000_000),
            }]);
            default_init(deps.as_mut());
            let env = mock_env();

            let msg_drand_worker_fee_percentage =
                msg_constructor_none(Proposal::DrandWorkerFeePercentage);
            let msg_lottery_every_block_time =
                msg_constructor_none(Proposal::LotteryEveryBlockTime);
            let msg_jackpot_reward_percentage =
                msg_constructor_none(Proposal::JackpotRewardPercentage);
            let msg_prize_per_rank = msg_constructor_none(Proposal::PrizesPerRanks);
            let msg_holder_fee_per_percentage = msg_constructor_none(Proposal::HolderFeePercentage);
            let msg_amount_to_register = msg_constructor_none(Proposal::AmountToRegister);
            let msg_security_migration = msg_constructor_none(Proposal::SecurityMigration);
            let msg_dao_funding = msg_constructor_none(Proposal::DaoFunding);
            let msg_staking_contract_migration =
                msg_constructor_none(Proposal::StakingContractMigration);

            let info = mock_info(
                before_all.default_sender.as_str().clone(),
                &[Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128(100_000_000),
                }],
            );
            let res = execute(deps.as_mut(), env.clone(), info.clone(), msg_dao_funding);
            match res {
                Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "Amount required"),
                _ => panic!("Unexpected error"),
            }

            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_security_migration,
            );
            match res {
                Err(StdError::GenericErr { msg, .. }) => {
                    assert_eq!(msg, "Migration is required")
                }
                _ => panic!("Unexpected error"),
            }

            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_staking_contract_migration,
            );
            match res {
                Err(StdError::GenericErr { msg, .. }) => {
                    assert_eq!(msg, "Migration address is required")
                }
                _ => panic!("Unexpected error"),
            }

            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_lottery_every_block_time,
            );
            match res {
                Err(StdError::GenericErr { msg, .. }) => {
                    assert_eq!(msg, "Amount block time required")
                }
                _ => panic!("Unexpected error"),
            }

            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_drand_worker_fee_percentage,
            );
            match res {
                Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "Amount is required"),
                _ => panic!("Unexpected error"),
            }

            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_jackpot_reward_percentage,
            );
            match res {
                Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "Amount is required"),
                _ => panic!("Unexpected error"),
            }

            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_holder_fee_per_percentage,
            );
            match res {
                Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "Amount is required"),
                _ => panic!("Unexpected error"),
            }

            let res = execute(deps.as_mut(), env.clone(), info.clone(), msg_prize_per_rank);
            match res {
                Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "Rank is required"),
                _ => panic!("Unexpected error"),
            }

            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_amount_to_register,
            );
            match res {
                Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "Amount is required"),
                _ => panic!("Unexpected error"),
            }

            let msg_drand_worker_fee_percentage =
                msg_constructor_amount_out(Proposal::DrandWorkerFeePercentage);
            let msg_jackpot_reward_percentage =
                msg_constructor_amount_out(Proposal::JackpotRewardPercentage);
            let msg_holder_fee_per_percentage =
                msg_constructor_amount_out(Proposal::HolderFeePercentage);

            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_drand_worker_fee_percentage,
            );
            println!("{:?}", res);
            match res {
                Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "Amount between 0 to 10"),
                _ => panic!("Unexpected error"),
            }

            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_jackpot_reward_percentage,
            );
            println!("{:?}", res);
            match res {
                Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "Amount between 0 to 100"),
                _ => panic!("Unexpected error"),
            }

            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_holder_fee_per_percentage,
            );
            println!("{:?}", res);
            match res {
                Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "Amount between 0 to 20"),
                _ => panic!("Unexpected error"),
            }

            let msg_prize_per_rank = msg_constructor_prize_len_out(Proposal::PrizesPerRanks);
            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_prize_per_rank.clone(),
            );
            println!("{:?}", res);
            match res {
                Err(StdError::GenericErr { msg, .. }) => assert_eq!(
                    msg,
                    "Ranks need to be in this format [0, 90, 10, 0] numbers between 0 to 100"
                ),
                _ => panic!("Unexpected error"),
            }
            let msg_prize_per_rank = msg_constructor_prize_sum_out(Proposal::PrizesPerRanks);
            let res = execute(deps.as_mut(), env.clone(), info.clone(), msg_prize_per_rank);
            println!("{:?}", res);
            match res {
                Err(StdError::GenericErr { msg, .. }) => {
                    assert_eq!(msg, "Numbers total sum need to be equal to 100")
                }
                _ => panic!("Unexpected error"),
            }
        }
        fn msg_constructor_success(
            proposal: Proposal,
            amount: Option<Uint128>,
            prizes_per_ranks: Option<Vec<u8>>,
            recipient: Option<String>,
            migration: Option<Migration>,
        ) -> ExecuteMsg {
            ExecuteMsg::Poll {
                description: "This is my first proposal".to_string(),
                proposal,
                amount,
                recipient,
                prizes_per_ranks,
                migration: migration,
                contract_address: "lottery".to_string(),
            }
        }

        #[test]
        fn success() {
            let before_all = before_all();
            let mut deps = mock_dependencies_custom(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128(9_000_000),
            }]);
            deps.querier.with_token_balances(Uint128(200_000));
            default_init(deps.as_mut());
            let state = STATE.load(deps.as_ref().storage).unwrap();
            assert_eq!(state.poll_id, 0);
            let env = mock_env();
            let info = mock_info(
                before_all.default_sender.as_str().clone(),
                &[Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128(100_000_000),
                }],
            );
            let msg_lottery_every_block_time = msg_constructor_success(
                Proposal::LotteryEveryBlockTime,
                Option::from(Uint128(22)),
                None,
                None,
                None,
            );
            let msg_amount_to_register = msg_constructor_success(
                Proposal::AmountToRegister,
                Option::from(Uint128(22)),
                None,
                None,
                None,
            );
            let msg_holder_fee_percentage = msg_constructor_success(
                Proposal::HolderFeePercentage,
                Option::from(Uint128(20)),
                None,
                None,
                None,
            );
            let msg_prize_rank = msg_constructor_success(
                Proposal::PrizesPerRanks,
                None,
                Option::from(vec![10, 10, 10, 70]),
                None,
                None,
            );
            let msg_jackpot_reward_percentage = msg_constructor_success(
                Proposal::JackpotRewardPercentage,
                Option::from(Uint128(80)),
                None,
                None,
                None,
            );
            let msg_drand_fee_worker = msg_constructor_success(
                Proposal::DrandWorkerFeePercentage,
                Option::from(Uint128(10)),
                None,
                None,
                None,
            );
            let msg_security_migration = msg_constructor_success(
                Proposal::SecurityMigration,
                None,
                None,
                Some(before_all.default_sender_two.clone()),
                Some(Migration {
                    contract_addr: "new".to_string(),
                    new_code_id: 1,
                    msg: Default::default(),
                }),
            );
            let msg_dao_funding = msg_constructor_success(
                Proposal::DaoFunding,
                Option::from(Uint128(200_000)),
                None,
                None,
                None,
            );

            let msg_staking_contract_migration = msg_constructor_success(
                Proposal::StakingContractMigration,
                None,
                None,
                Option::from(before_all.default_sender_two.clone()),
                None,
            );

            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_lottery_every_block_time,
            )
            .unwrap();
            assert_eq!(res.attributes.len(), 3);
            let poll_state = POLL
                .load(deps.as_ref().storage, &1_u64.to_be_bytes())
                .unwrap();
            assert_eq!(
                poll_state.creator,
                deps.api
                    .addr_canonicalize(&before_all.default_sender)
                    .unwrap()
            );
            let state = STATE.load(deps.as_ref().storage).unwrap();
            assert_eq!(state.poll_id, 1);

            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_amount_to_register,
            )
            .unwrap();
            assert_eq!(res.attributes.len(), 3);
            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_holder_fee_percentage,
            )
            .unwrap();
            assert_eq!(res.attributes.len(), 3);
            let res = execute(deps.as_mut(), env.clone(), info.clone(), msg_prize_rank).unwrap();
            assert_eq!(res.attributes.len(), 3);
            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_jackpot_reward_percentage,
            )
            .unwrap();
            assert_eq!(res.attributes.len(), 3);
            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_drand_fee_worker,
            )
            .unwrap();
            assert_eq!(res.attributes.len(), 3);

            // Admin create proposal migration
            let env = mock_env();
            let info = mock_info(
                before_all.default_sender_owner.as_str().clone(),
                &[Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128(100_000_000),
                }],
            );
            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_security_migration.clone(),
            )
            .unwrap();
            assert_eq!(res.attributes.len(), 3);
            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_staking_contract_migration.clone(),
            )
            .unwrap();
            assert_eq!(res.attributes.len(), 3);
            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_dao_funding.clone(),
            )
            .unwrap();
            assert_eq!(res.attributes.len(), 3);

            let env = mock_env();
            let info = mock_info(
                before_all.default_sender.as_str().clone(),
                &[Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128(100_000_000),
                }],
            );
            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_security_migration.clone(),
            )
            .unwrap();
            assert_eq!(res.attributes.len(), 3);
            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_staking_contract_migration.clone(),
            )
            .unwrap();
            assert_eq!(res.attributes.len(), 3);
            let res = execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                msg_dao_funding.clone(),
            )
            .unwrap();
            assert_eq!(res.attributes.len(), 3);
        }
    }
    mod vote {
        use super::*;
        use crate::state::{PollInfoState, Proposal};
        use cosmwasm_std::{Api, Coin, Decimal, Event};

        // handle_vote
        fn create_poll(deps: DepsMut) {
            let msg = ExecuteMsg::Poll {
                description: "This is my first proposal".to_string(),
                proposal: Proposal::LotteryEveryBlockTime,
                amount: Option::from(Uint128(22)),
                prizes_per_ranks: None,
                recipient: None,
                migration: None,
                contract_address: "lottery".to_string(),
            };

            let _res = execute(
                deps,
                mock_env(),
                mock_info(
                    "addr0000",
                    &[Coin {
                        denom: "uusd".to_string(),
                        amount: Uint128(100),
                    }],
                ),
                msg,
            )
            .unwrap();
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
    mod reject {
        use super::*;
        use cosmwasm_std::{Coin, Event};

        // handle_reject
        fn create_poll(deps: DepsMut) {
            let msg = ExecuteMsg::Poll {
                description: "This is my first proposal".to_string(),
                proposal: Proposal::LotteryEveryBlockTime,
                amount: Option::from(Uint128(22)),
                recipient: None,
                prizes_per_ranks: None,
                migration: None,
                contract_address: "lottery".to_string(),
            };
            let _res = execute(
                deps,
                mock_env(),
                mock_info(
                    "addr0000",
                    &[Coin {
                        denom: "uusd".to_string(),
                        amount: Uint128(100_000_000),
                    }],
                ),
                msg,
            )
            .unwrap();
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
                    amount: Uint128(1_000),
                }],
            );
            let msg = ExecuteMsg::RejectPoll { poll_id: 1 };
            let res = execute(deps.as_mut(), env, info, msg);
            println!("{:?}", res);
            match res {
                Err(StdError::GenericErr { msg, .. }) => {
                    assert_eq!(msg, "Do not send funds with reject proposal")
                }
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
            let msg = ExecuteMsg::RejectPoll { poll_id: 1 };
            let res = execute(deps.as_mut(), env, info, msg);
            println!("{:?}", res);
            match res {
                Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "Proposal expired"),
                _ => panic!("Unexpected error"),
            }
        }
        #[test]
        fn only_creator_can_reject() {
            let before_all = before_all();
            let mut deps = mock_dependencies(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128(9_000_000),
            }]);
            default_init(deps.as_mut());
            create_poll(deps.as_mut());
            let msg = ExecuteMsg::RejectPoll { poll_id: 1 };
            let env = mock_env();
            let info = mock_info(before_all.default_sender_two.as_str().clone(), &[]);
            let res = execute(deps.as_mut(), env, info, msg);

            println!("{:?}", res);
            match res {
                Err(StdError::GenericErr { msg, .. }) => {
                    assert_eq!(msg, "Unauthorized");
                }
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
            default_init(deps.as_mut());
            create_poll(deps.as_mut());
            let result = ContractResult::Ok(SubcallResponse {
                events: vec![Event {
                    kind: "instantiate_contract".to_string(),
                    attributes: vec![attr("contract_address", "loterra")],
                }],
                data: None,
            });
            let reply = reply(deps.as_mut(), mock_env(), Reply { id: 0, result }).unwrap();
            let msg = ExecuteMsg::RejectPoll { poll_id: 1 };
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
            println!("{:?}", res);
            assert_eq!(res.messages.len(), 1);
            assert_eq!(res.attributes.len(), 2);
            let poll_state = POLL
                .load(deps.as_ref().storage, &1_u64.to_be_bytes())
                .unwrap();
            assert_eq!(poll_state.status, PollStatus::RejectedByCreator);
        }
    }

    mod present {
        use super::*;
        use cosmwasm_std::{Attribute, BankMsg, Coin, CosmosMsg, Decimal, Event};

        // handle_present
        fn create_poll(deps: DepsMut) {
            let msg = ExecuteMsg::Poll {
                description: "This is my first proposal".to_string(),
                proposal: Proposal::LotteryEveryBlockTime,
                amount: Some(Uint128(22)),
                recipient: None,
                prizes_per_ranks: None,
                migration: None,
                contract_address: "lottery".to_string(),
            };
            let _res = execute(
                deps,
                mock_env(),
                mock_info(
                    "addr0000",
                    &[Coin {
                        denom: "uusd".to_string(),
                        amount: Uint128(100_000_000),
                    }],
                ),
                msg,
            )
            .unwrap();
        }
        fn create_poll_security_migration(deps: DepsMut) {
            let msg = ExecuteMsg::Poll {
                description: "This is my first proposal".to_string(),
                proposal: Proposal::SecurityMigration,
                amount: None,
                recipient: Some("newAddress".to_string()),
                prizes_per_ranks: None,
                migration: Some(Migration {
                    contract_addr: "new".to_string(),
                    new_code_id: 0,
                    msg: Default::default(),
                }),
                contract_address: "lottery".to_string(),
            };
            let _res = execute(
                deps,
                mock_env(),
                mock_info(
                    "addr0002",
                    &[Coin {
                        denom: "uusd".to_string(),
                        amount: Uint128(100_000_000),
                    }],
                ),
                msg,
            )
            .unwrap();
            println!("{:?}", _res);
        }
        fn create_poll_dao_funding(deps: DepsMut) {
            let msg = ExecuteMsg::Poll {
                description: "This is my first proposal".to_string(),
                proposal: Proposal::DaoFunding,
                amount: Some(Uint128(22)),
                recipient: None,
                prizes_per_ranks: None,
                migration: None,
                contract_address: "lottery".to_string(),
            };
            let _res = execute(
                deps,
                mock_env(),
                mock_info(
                    "addr0002",
                    &[Coin {
                        denom: "uusd".to_string(),
                        amount: Uint128(100_000_000),
                    }],
                ),
                msg,
            )
            .unwrap();
        }
        fn create_poll_statking_contract_migration(deps: DepsMut) {
            let msg = ExecuteMsg::Poll {
                description: "This is my first proposal".to_string(),
                proposal: Proposal::StakingContractMigration,
                amount: None,
                recipient: Some("newAddress".to_string()),
                prizes_per_ranks: None,
                migration: None,
                contract_address: "lottery".to_string(),
            };
            let _res = execute(
                deps,
                mock_env(),
                mock_info(
                    "addr0002",
                    &[Coin {
                        denom: "uusd".to_string(),
                        amount: Uint128(100_000_000),
                    }],
                ),
                msg,
            )
            .unwrap();
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
            let msg = ExecuteMsg::PresentPoll { poll_id: 1 };
            let res = execute(deps.as_mut(), env, info, msg);
            println!("{:?}", res);
            match res {
                Err(StdError::GenericErr { msg, .. }) => {
                    assert_eq!(msg, "Do not send funds with present proposal")
                }
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
            // Save to storage

            POLL.update(
                deps.as_mut().storage,
                &1_u64.to_be_bytes(),
                |poll| -> StdResult<PollInfoState> {
                    match poll {
                        None => panic!("Unexpected error"),
                        Some(poll_state) => {
                            let mut poll_data = poll_state;
                            // Update the status to passed
                            poll_data.status = PollStatus::Rejected;
                            Ok(poll_data)
                        }
                    }
                },
            )
            .unwrap();
            let env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let msg = ExecuteMsg::PresentPoll { poll_id: 1 };
            let res = execute(deps.as_mut(), env, info, msg);
            println!("{:?}", res);
            match res {
                Err(StdError::GenericErr { msg, .. }) => {
                    assert_eq!(msg, "Unauthorized")
                }
                _ => panic!("Unexpected error"),
            }
        }
        #[test]
        fn poll_still_in_progress() {
            let before_all = before_all();
            let mut deps = mock_dependencies_custom(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128(9_000_000),
            }]);

            default_init(deps.as_mut());
            create_poll(deps.as_mut());

            let env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let msg = ExecuteMsg::PresentPoll { poll_id: 1 };
            POLL.update(
                deps.as_mut().storage,
                &1_u64.to_be_bytes(),
                |poll| -> StdResult<PollInfoState> {
                    match poll {
                        None => panic!("Unexpected error"),
                        Some(poll_state) => {
                            let mut poll_data = poll_state;
                            // Update the status to passed
                            poll_data.end_height = env.block.height + 100;
                            Ok(poll_data)
                        }
                    }
                },
            )
            .unwrap();
            let res = execute(deps.as_mut(), env, info, msg);
            println!("{:?}", res);
            match res {
                Err(StdError::GenericErr { msg, .. }) => {
                    assert_eq!(msg, "Proposal still in progress")
                }
                _ => panic!("Unexpected error"),
            }
        }
        #[test]
        fn success_with_reject() {
            let before_all = before_all();
            let mut deps = mock_dependencies_custom(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128(9_000_000),
            }]);
            deps.querier.with_token_balances(Uint128(200_000));
            default_init(deps.as_mut());
            create_poll(deps.as_mut());

            let mut env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let poll_state = POLL
                .load(deps.as_ref().storage, &1_u64.to_be_bytes())
                .unwrap();
            env.block.height = poll_state.end_height + 1;

            let msg = ExecuteMsg::PresentPoll { poll_id: 1 };
            let res = execute(deps.as_mut(), env, info, msg).unwrap();
            assert_eq!(res.attributes.len(), 3);
            assert_eq!(res.messages.len(), 0);

            let poll_state = POLL
                .load(deps.as_ref().storage, &1_u64.to_be_bytes())
                .unwrap();
            assert_eq!(poll_state.status, PollStatus::Rejected);
        }

        #[test]
        fn success_with_passed() {
            let before_all = before_all();
            let mut deps = mock_dependencies_custom(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128(9_000_000),
            }]);
            //deps.querier.with_token_balances(Uint128(200_000));
            deps.querier.with_holder(
                before_all.default_sender.clone(),
                Uint128(100_000_000),
                Decimal::zero(),
                Decimal::zero(),
            );
            default_init(deps.as_mut());
            let env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            create_poll(deps.as_mut());

            let env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let msg = ExecuteMsg::Vote {
                poll_id: 1,
                approve: true,
            };

            let _res = execute(deps.as_mut(), env, info, msg);

            let mut env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let poll_state = POLL
                .load(deps.as_ref().storage, &1_u64.to_be_bytes())
                .unwrap();
            env.block.height = poll_state.end_height + 1;

            let msg = ExecuteMsg::PresentPoll { poll_id: 1 };
            let result = ContractResult::Ok(SubcallResponse {
                events: vec![Event {
                    kind: "instantiate_contract".to_string(),
                    attributes: vec![attr("contract_address", "loterra")],
                }],
                data: None,
            });
            let reply = reply(deps.as_mut(), env.clone(), Reply { id: 0, result }).unwrap();
            let res = execute(deps.as_mut(), env, info, msg).unwrap();
            assert_eq!(res.attributes.len(), 3);
            assert_eq!(res.messages.len(), 1);

            let poll_state = POLL
                .load(deps.as_ref().storage, &1_u64.to_be_bytes())
                .unwrap();
            assert_eq!(poll_state.status, PollStatus::Passed);

            let env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            create_poll_security_migration(deps.as_mut());
            let msg = ExecuteMsg::Vote {
                poll_id: 2,
                approve: true,
            };
            let res = execute(deps.as_mut(), env, info, msg).unwrap();
            println!("{:?}", res);
        }
        #[test]
        fn success_with_proposal_not_expired_yet_and_more_50_percent_weight_vote() {
            let before_all = before_all();
            let mut deps = mock_dependencies_custom(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128(9_000_000),
            }]);
            // deps.querier.with_token_balances(Uint128(200_000));
            deps.querier.with_holder(
                before_all.default_sender.clone(),
                Uint128(500_000_000),
                Decimal::zero(),
                Decimal::zero(),
            );
            default_init(deps.as_mut());
            let env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            create_poll(deps.as_mut());

            let result = ContractResult::Ok(SubcallResponse {
                events: vec![Event {
                    kind: "instantiate_contract".to_string(),
                    attributes: vec![attr("contract_address", "loterra")],
                }],
                data: None,
            });
            let reply = reply(deps.as_mut(), env.clone(), Reply { id: 0, result }).unwrap();
            let env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let msg = ExecuteMsg::Vote {
                poll_id: 1,
                approve: true,
            };

            let _res = execute(deps.as_mut(), env, info, msg);

            let mut env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let poll_state = POLL
                .load(deps.as_ref().storage, &1_u64.to_be_bytes())
                .unwrap();
            env.block.height = poll_state.end_height - 1000;

            let msg = ExecuteMsg::PresentPoll { poll_id: 1 };
            let res = execute(deps.as_mut(), env, info, msg).unwrap();
            assert_eq!(res.attributes.len(), 3);
            assert_eq!(res.messages.len(), 1);

            let poll_state = POLL
                .load(deps.as_ref().storage, &1_u64.to_be_bytes())
                .unwrap();
            assert_eq!(poll_state.status, PollStatus::Passed);

            let env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            create_poll_security_migration(deps.as_mut());
            let msg = ExecuteMsg::Vote {
                poll_id: 2,
                approve: true,
            };
            let res = execute(deps.as_mut(), env, info, msg).unwrap();
            println!("{:?}", res);
        }

        #[test]
        fn error_with_proposal_not_expired_yet_and_less_50_percent_weight_vote() {
            let before_all = before_all();
            let mut deps = mock_dependencies_custom(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128(9_000_000),
            }]);
            deps.querier.with_token_balances(Uint128(200_000));
            deps.querier.with_holder(
                before_all.default_sender.clone(),
                Uint128(1_000),
                Decimal::zero(),
                Decimal::zero(),
            );
            default_init(deps.as_mut());
            let env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            create_poll(deps.as_mut());

            let env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let msg = ExecuteMsg::Vote {
                poll_id: 1,
                approve: true,
            };

            let _res = execute(deps.as_mut(), env, info, msg);

            let mut env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let poll_state = POLL
                .load(deps.as_ref().storage, &1_u64.to_be_bytes())
                .unwrap();
            env.block.height = poll_state.end_height - 1000;

            let msg = ExecuteMsg::PresentPoll { poll_id: 1 };
            let result = ContractResult::Ok(SubcallResponse {
                events: vec![Event {
                    kind: "instantiate_contract".to_string(),
                    attributes: vec![attr("contract_address", "loterra")],
                }],
                data: None,
            });
            let reply = reply(deps.as_mut(), env.clone(), Reply { id: 0, result }).unwrap();
            let res = execute(deps.as_mut(), env, info, msg);
            match res {
                Err(StdError::GenericErr { msg, .. }) => {
                    assert_eq!(msg, "Proposal still in progress")
                }
                _ => panic!("Unexpected error"),
            }
        }
        #[test]
        fn poll_expired_but_quorum_not_reached() {
            let before_all = before_all();
            let mut deps = mock_dependencies_custom(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128(9_000_000),
            }]);
            //deps.querier.with_token_balances(Uint128(200_000));
            deps.querier.with_holder(
                before_all.default_sender.clone(),
                Uint128(99_000_000),
                Decimal::zero(),
                Decimal::zero(),
            );
            default_init(deps.as_mut());
            let env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            create_poll(deps.as_mut());

            let env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let msg = ExecuteMsg::Vote {
                poll_id: 1,
                approve: true,
            };

            let _res = execute(deps.as_mut(), env, info, msg);

            let mut env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let poll_state = POLL
                .load(deps.as_ref().storage, &1_u64.to_be_bytes())
                .unwrap();
            env.block.height = poll_state.end_height + 1000;

            let msg = ExecuteMsg::PresentPoll { poll_id: 1 };
            let res = execute(deps.as_mut(), env, info, msg).unwrap();
            let poll_state = POLL
                .load(deps.as_ref().storage, &1_u64.to_be_bytes())
                .unwrap();
            assert_eq!(poll_state.status, PollStatus::Rejected);
        }
        #[test]
        fn poll_expired_and_quorum_reached() {
            let before_all = before_all();
            let mut deps = mock_dependencies_custom(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128(9_000_000),
            }]);
            //deps.querier.with_token_balances(Uint128(200_000));
            deps.querier.with_holder(
                before_all.default_sender.clone(),
                Uint128(100_000_000),
                Decimal::zero(),
                Decimal::zero(),
            );
            default_init(deps.as_mut());
            let env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            create_poll(deps.as_mut());
            let result = ContractResult::Ok(SubcallResponse {
                events: vec![Event {
                    kind: "instantiate_contract".to_string(),
                    attributes: vec![attr("contract_address", "loterra")],
                }],
                data: None,
            });
            let reply = reply(deps.as_mut(), env.clone(), Reply { id: 0, result }).unwrap();
            let env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let msg = ExecuteMsg::Vote {
                poll_id: 1,
                approve: true,
            };

            let _res = execute(deps.as_mut(), env, info, msg);

            let mut env = mock_env();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let poll_state = POLL
                .load(deps.as_ref().storage, &1_u64.to_be_bytes())
                .unwrap();
            env.block.height = poll_state.end_height + 1000;

            let msg = ExecuteMsg::PresentPoll { poll_id: 1 };
            let res = execute(deps.as_mut(), env, info, msg).unwrap();
            let poll_state = POLL
                .load(deps.as_ref().storage, &1_u64.to_be_bytes())
                .unwrap();
            assert_eq!(poll_state.status, PollStatus::Passed);
        }
        #[test]
        fn reply_lottery() {
            let before_all = before_all();
            let mut deps = mock_dependencies_custom(&[Coin {
                denom: "ust".to_string(),
                amount: Uint128(9_000_000),
            }]);
            deps.querier.with_holder(
                before_all.default_sender.clone(),
                Uint128(100_000_000),
                Decimal::zero(),
                Decimal::zero(),
            );
            default_init(deps.as_mut());
            create_poll(deps.as_mut());
            let mut env = mock_env();
            let result = ContractResult::Ok(SubcallResponse {
                events: vec![Event {
                    kind: "instantiate_contract".to_string(),
                    attributes: vec![attr("contract_address", "loterra")],
                }],
                data: None,
            });
            let rep = reply(deps.as_mut(), env.clone(), Reply { id: 0, result }).unwrap();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let msg = ExecuteMsg::Vote {
                poll_id: 1,
                approve: true,
            };
            execute(deps.as_mut(), env.clone(), info, msg).unwrap();
            let info = mock_info(before_all.default_sender.as_str().clone(), &[]);
            let poll_state = POLL
                .load(deps.as_ref().storage, &1_u64.to_be_bytes())
                .unwrap();
            env.block.height = poll_state.end_height + 1000;

            let msg = ExecuteMsg::PresentPoll { poll_id: 1 };
            let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

            // Reply applied true
            let rep = Reply {
                id: 1,
                result: ContractResult::Ok(SubcallResponse {
                    events: vec![Event {
                        kind: "message".to_string(),
                        attributes: vec![
                            attr("action", "apply poll"),
                            attr("applied", true),
                            attr("poll_id", 1),
                        ],
                    }],
                    data: None,
                }),
            };
            let res = reply(deps.as_mut(), env.clone(), rep).unwrap();
            let poll = POLL
                .load(deps.as_ref().storage, &1_u64.to_be_bytes())
                .unwrap();
            // true
            assert!(poll.applied);

            // Reply applied false
            let rep = Reply {
                id: 1,
                result: ContractResult::Ok(SubcallResponse {
                    events: vec![Event {
                        kind: "message".to_string(),
                        attributes: vec![
                            attr("action", "apply poll"),
                            attr("applied", false),
                            attr("poll_id", 1),
                        ],
                    }],
                    data: None,
                }),
            };
            let res = reply(deps.as_mut(), env, rep).unwrap();
            let poll = POLL
                .load(deps.as_ref().storage, &1_u64.to_be_bytes())
                .unwrap();
            // false
            assert!(!poll.applied);
        }
    }
}

//let result = ContractResult::Ok(SubcallResponse{ events: vec![Event{ kind: "instantiate_contract".to_string(), attributes: vec![attr("contract_address", "loterra")] }], data: None });
//let reply = reply(deps.as_mut(), env.clone(), Reply{ id: 0, result }).unwrap();
