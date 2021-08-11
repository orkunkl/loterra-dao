use crate::error::ContractError;
use crate::msg::{HolderResponse, LoterraStaking, StakingStateResponse};
use crate::state::{Config, PollStatus, POLL};
use cosmwasm_std::{attr, to_binary, Addr, DepsMut, Response, StdResult, Uint128, WasmQuery};

pub fn reject_proposal(deps: DepsMut, poll_id: u64) -> Result<Response, ContractError> {
    POLL.update(
        deps.storage,
        &poll_id.to_be_bytes(),
        |poll| -> StdResult<_> {
            let mut update_poll = poll.unwrap();
            update_poll.status = PollStatus::Rejected;
            Ok(update_poll)
        },
    )?;
    let res = Response::new()
        .add_attribute("action", "present the proposal")
        .add_attribute("proposal_id", poll_id.to_string())
        .add_attribute("proposal_result", "rejected");
    Ok(res)
}

pub fn user_total_weight(deps: &DepsMut, config: &Config, address: &Addr) -> Uint128 {
    let mut weight = Uint128::zero();

    // Ensure sender have some reward tokens
    let msg = LoterraStaking::Holder {
        address: address.to_string(),
    };

    let loterra_human = &config.staking_contract_address;

    let query = WasmQuery::Smart {
        contract_addr: loterra_human.to_string(),
        msg: to_binary(&msg).unwrap(),
    }
    .into();

    let loterra_balance: HolderResponse = deps.querier.query(&query).unwrap();
    if !loterra_balance.balance.is_zero() {
        weight += loterra_balance.balance;
    }

    weight
}

pub fn total_weight(deps: &DepsMut, config: &Config) -> Uint128 {
    let msg = LoterraStaking::State {};
    let loterra_human = &config.staking_contract_address;
    let query = WasmQuery::Smart {
        contract_addr: loterra_human.to_string(),
        msg: to_binary(&msg).unwrap(),
    }
    .into();

    let loterra_balance: StakingStateResponse = deps.querier.query(&query).unwrap();
    loterra_balance.total_balance
}
