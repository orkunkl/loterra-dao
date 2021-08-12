#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, to_binary, Addr, BankMsg, Binary, Coin, ContractResult, CosmosMsg, Deps, DepsMut, Env,
    MessageInfo, Reply, ReplyOn, Response, StdError, StdResult, SubMsg, SubMsgExecutionResponse,
    Uint128, WasmMsg, WasmQuery,
};
use cw20::{BalanceResponse, Cw20QueryMsg};
use std::ops::Add;

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

const MAX_DESCRIPTION_LEN: u64 = 255;
const MIN_DESCRIPTION_LEN: u64 = 6;
const HOLDERS_MAX_REWARD: u8 = 20;
const WORKER_MAX_REWARD: u8 = 10;
const YES_WEIGHT: u128 = 50;
const NO_WEIGHT: u128 = 33;
const QUORUM: u128 = 10;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    let config = Config {
        admin: deps.api.addr_validate(info.sender.as_str())?,
        poll_default_end_height: msg.poll_default_end_height,
        staking_contract_address: deps
            .api
            .addr_validate(msg.staking_contract_address.as_str())?,
        cw20_contract_address: deps.api.addr_validate(msg.cw20_contract_address.as_str())?,
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
        admin: Some(env.contract.address.to_string()),
        code_id: msg.code_id,
        msg: msg.message,
        label: msg.label,
        funds: vec![],
    };
    let sub_message = SubMsg {
        id: 0,
        msg: CosmosMsg::Wasm(wasm_msg),
        gas_limit: None,
        reply_on: ReplyOn::Success,
    };

    let res = Response::new()
        .add_submessage(sub_message)
        .add_attribute("action", "instantiate")
        .add_attribute("admin", info.sender)
        .add_attribute("code_id", msg.code_id.to_string());
    Ok(res)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
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
    prizes_per_ranks: Option<Vec<u64>>,
    amount: Option<Uint128>,
    recipient: Option<String>,
    migration: Option<Migration>,
    contract_address: String,
) -> Result<Response, ContractError> {
    let mut state = STATE.load(deps.storage)?;
    let config = CONFIG.load(deps.storage)?;

    // Increment and get the new poll id for bucket key
    let poll_id = state.poll_id.checked_add(1).unwrap();
    // Set the new counter
    state.poll_id = poll_id;

    // Check if some funds are sent
    let sent = match info.funds.len() {
        0 => Err(ContractError::RequiredCollateral(state.required_collateral)),
        1 => {
            if info.funds[0].denom == state.denom {
                Ok(info.funds[0].amount)
            } else {
                Err(ContractError::RequiredCollateral(state.required_collateral))
            }
        }
        _ => Err(ContractError::RequiredCollateral(state.required_collateral)),
    }?;

    // Handle the description is respecting length
    if (description.len() as u64) < MIN_DESCRIPTION_LEN
        || (description.len() as u64) > MAX_DESCRIPTION_LEN
    {
        return Err(ContractError::WrongDescLength(
            description.len(),
            MIN_DESCRIPTION_LEN,
            MAX_DESCRIPTION_LEN,
        ));
    }

    let mut proposal_amount: Uint128 = Uint128::zero();
    let mut proposal_prize_rank: Vec<u64> = vec![];
    let mut proposal_human_address: Option<String> = None;
    let mut migration_to: Option<Migration> = None;

    let proposal_type = if let Proposal::HolderFeePercentage = proposal {
        match amount {
            Some(percentage) => {
                if percentage.u128() as u8 > HOLDERS_MAX_REWARD {
                    return Err(ContractError::MaxReward(HOLDERS_MAX_REWARD));
                }
                proposal_amount = percentage;
            }
            None => return Err(ContractError::InvalidAmount()),
        }

        Proposal::HolderFeePercentage
    } else if let Proposal::DrandWorkerFeePercentage = proposal {
        match amount {
            Some(percentage) => {
                if percentage.u128() as u8 > WORKER_MAX_REWARD {
                    return Err(ContractError::MaxReward(WORKER_MAX_REWARD));
                }
                proposal_amount = percentage;
            }
            None => return Err(ContractError::InvalidAmount()),
        }

        Proposal::DrandWorkerFeePercentage
    } else if let Proposal::JackpotRewardPercentage = proposal {
        match amount {
            Some(percentage) => {
                if percentage.u128() as u8 > 100 {
                    return Err(ContractError::InvalidAmount());
                }
                proposal_amount = percentage;
            }
            None => return Err(ContractError::InvalidAmount()),
        }

        Proposal::JackpotRewardPercentage
    } else if let Proposal::LotteryEveryBlockTime = proposal {
        match amount {
            Some(block_time) => {
                proposal_amount = block_time;
            }
            None => {
                return Err(ContractError::InvalidBlockTime());
            }
        }

        Proposal::LotteryEveryBlockTime
    } else if let Proposal::PrizesPerRanks = proposal {
        match prizes_per_ranks {
            Some(ranks) => {
                if ranks.len() != 6 {
                    return Err(ContractError::InvalidRank());
                }
                let mut total_percentage = 0;
                for rank in ranks.clone() {
                    if (rank as u64) > 1000 {
                        return Err(ContractError::InvalidNumber());
                    }
                    total_percentage += rank;
                }
                // Ensure the repartition sum is 100%
                if total_percentage != 1000 {
                    return Err(ContractError::InvalidNumberSum());
                }

                proposal_prize_rank = ranks;
            }
            None => return Err(ContractError::InvalidRank()),
        }
        Proposal::PrizesPerRanks
    } else if let Proposal::AmountToRegister = proposal {
        match amount {
            Some(amount_to_register) => {
                proposal_amount = amount_to_register;
            }
            None => return Err(ContractError::InvalidAmount()),
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
            None => return Err(ContractError::InvalidMigration()),
        }
        Proposal::SecurityMigration
    } else if let Proposal::DaoFunding = proposal {
        match amount {
            Some(amount) => {
                if amount.is_zero() {
                    return Err(ContractError::InvalidAmount());
                }

                // Get the contract balance prepare the tx
                let msg_balance = Cw20QueryMsg::Balance {
                    address: env.contract.address.to_string(),
                };

                let res_balance = WasmQuery::Smart {
                    contract_addr: config.cw20_contract_address.into_string(),
                    msg: to_binary(&msg_balance)?,
                };
                let loterra_balance: BalanceResponse = deps.querier.query(&res_balance.into())?;

                if loterra_balance.balance.is_zero()
                    || loterra_balance.balance.u128() < amount.u128()
                {
                    return Err(ContractError::NotEnoughFunds {});
                }

                proposal_amount = amount;
                proposal_human_address = recipient;
            }
            None => return Err(ContractError::InvalidAmount()),
        }
        Proposal::DaoFunding
    } else if let Proposal::StakingContractMigration = proposal {
        match recipient {
            Some(recipient) => {
                proposal_human_address = Some(recipient);
            }
            None => return Err(ContractError::NoMigrationAddress()),
        }
        Proposal::StakingContractMigration
    } else if let Proposal::PollSurvey = proposal {
        Proposal::PollSurvey
    } else {
        return Err(ContractError::UnknownProposalType());
    };

    let sender_to_canonical = deps.api.addr_canonicalize(info.sender.as_str())?;

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
        contract_address: deps.api.addr_validate(contract_address.as_str())?,
        applied: false,
    };

    // Save poll
    POLL.save(deps.storage, &state.poll_id.to_be_bytes(), &new_poll)?;

    // Save state
    STATE.save(deps.storage, &state)?;

    let res = Response::new()
        .add_attribute("action", "create poll")
        .add_attribute("poll_id", poll_id.to_string())
        .add_attribute("poll_creation_result", "success");
    Ok(res)
}
pub fn try_vote(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    poll_id: u64,
    approve: bool,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    let mut poll = POLL.load(deps.storage, &poll_id.to_be_bytes())?;
    // Ensure the sender not sending funds accidentally
    if !info.funds.is_empty() {
        return Err(ContractError::DoNotSendFunds {});
    }
    let sender = deps.api.addr_canonicalize(info.sender.as_ref())?;

    // Ensure the poll is still valid
    if env.block.height > poll.end_height {
        return Err(ContractError::PollExpired {});
    }
    // Ensure the poll is still valid
    if poll.status != PollStatus::InProgress {
        return Err(ContractError::PollClosed {});
    }

    POLL_VOTE.update(
        deps.storage,
        (&poll_id.to_be_bytes(), &sender),
        |exist| match exist {
            None => Ok(approve),
            Some(_) => Err(ContractError::AlreadyVoted {}),
        },
    )?;

    // Get the sender weight
    let weight = user_total_weight(&deps, &config, &info.sender);
    //let weight = Uint128(200);
    // Only stakers can vote
    if weight.is_zero() {
        return Err(ContractError::OnlyStakersVote {});
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

    let res = Response::new()
        .add_attribute("action", "vote")
        .add_attribute("proposal_id", poll_id.to_string())
        .add_attribute("voting_result", "success");
    Ok(res)
}

fn try_reject(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    poll_id: u64,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    let poll = POLL.load(deps.storage, &poll_id.to_be_bytes())?;
    let sender = deps.api.addr_canonicalize(info.sender.as_str())?;

    // Ensure the sender not sending funds accidentally
    if !info.funds.is_empty() {
        return Err(ContractError::DoNotSendFunds {});
    }
    // Ensure end proposal height is not expired
    if poll.end_height < env.block.height {
        return Err(ContractError::ProposalExpired {});
    }
    // Ensure only the creator can reject a proposal OR the status of the proposal is still in progress
    if poll.creator != sender || poll.status != PollStatus::InProgress {
        return Err(ContractError::Unauthorized {});
    }

    POLL.update(deps.storage, &poll_id.to_be_bytes(), |poll| match poll {
        None => Err(ContractError::PollNotFound {}),
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
        // TODO: fix unwrap
        to_address: state.loterry_address.unwrap().into_string(),
        amount: vec![deduct_tax(
            &deps.querier,
            Coin {
                denom: state.denom,
                amount: poll.collateral,
            },
        )?],
    };

    let res = Response::new()
        .add_message(msg)
        .add_attribute("action", "creator reject the proposal")
        .add_attribute("poll_id", poll_id.to_string());
    Ok(res)
}

fn try_present(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    poll_id: u64,
) -> Result<Response, ContractError> {
    // Load storage
    let state = STATE.load(deps.storage)?;
    let config = CONFIG.load(deps.storage)?;
    let poll = POLL.load(deps.storage, &poll_id.to_be_bytes())?;

    // Ensure the sender not sending funds accidentally
    if !info.funds.is_empty() {
        return Err(ContractError::DoNotSendFunds {});
    }
    // Ensure the proposal is still in Progress
    if poll.status != PollStatus::InProgress {
        return Err(ContractError::Unauthorized {});
    }
    let query_total_stake = LoterraStaking::State {};
    let query = WasmQuery::Smart {
        contract_addr: config.staking_contract_address.to_string(),
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
            return Err(ContractError::ProposalInProgress {});
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
    let mut msg: Vec<CosmosMsg<_>> = vec![];
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
                // TODO: fix this
                to_address: state.loterry_address.unwrap().into_string(),
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
        contract_addr: poll.contract_address.into_string(),
        msg: to_binary(&sub_msg)?,
        funds: vec![],
    };
    let sub = SubMsg {
        id: 1,
        msg: CosmosMsg::Wasm(execute_sub_msg),
        gas_limit: None,
        reply_on: ReplyOn::Always,
    };

    let res = Response::new()
        .add_submessage(sub)
        .add_messages(msg)
        .add_attribute("action", "present poll")
        .add_attribute("poll_id", poll_id.to_string())
        .add_attribute("poll_result", "approved");
    Ok(res)
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
    msg: ContractResult<SubMsgExecutionResponse>,
) -> Result<Response, ContractError> {
    let mut state = STATE.load(deps.storage)?;
    /*
       Save the address of LoTerra contract lottery to the state
    */
    match msg {
        ContractResult::Ok(subcall) => {
            let contract_address: Addr = subcall
                .events
                .into_iter()
                .find(|e| e.ty == "instantiate_contract")
                .and_then(|ev| {
                    ev.attributes
                        .into_iter()
                        .find(|attr| attr.key == "contract_address")
                        .map(|addr| addr.value)
                })
                .and_then(|addr| deps.api.addr_validate(addr.as_str()).ok())
                // TODO: fix unwrap
                .unwrap();
            state.loterry_address = Some(contract_address.clone());
            STATE.save(deps.storage, &state)?;

            // Probably not possible need to check
            let update = WasmMsg::UpdateAdmin {
                contract_addr: contract_address.to_string(),
                admin: contract_address.to_string(),
            };

            let res = Response::new()
                .add_message(update)
                .add_attribute("lottery-address", contract_address.clone())
                .add_attribute("lottery-instantiate", "success")
                .add_attribute("lottery-update-admin", contract_address);
            Ok(res)
        }
        ContractResult::Err(_) => Err(ContractError::Unauthorized {}),
    }
}

pub fn loterra_lottery_reply(
    deps: DepsMut,
    _env: Env,
    msg: ContractResult<SubMsgExecutionResponse>,
) -> Result<Response, ContractError> {
    match msg {
        ContractResult::Ok(subcall) => {
            let (poll_result, poll_id) = subcall
                .events
                .into_iter()
                .find(|e| e.ty == "message")
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

            let res = Response::new()
                .add_attributes(vec![attr("applied", poll_result), attr("poll_id", poll_id)]);
            Ok(res)
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
        contract_address: poll.contract_address,
        applied: poll.applied,
    })
}
