use crate::state::{PollStatus, Config, POLL, STATE};
use cosmwasm_std::{attr, Addr, DepsMut, Response, StdResult, Uint128, WasmQuery, to_binary};
use crate::msg::{LoterraStaking, HolderResponse};

pub fn reject_proposal(deps: DepsMut, poll_id: u64) -> StdResult<Response> {
    POLL.update(
        deps.storage,
        &poll_id.to_be_bytes(),
        |poll| -> StdResult<_> {
            let mut update_poll = poll.unwrap();
            update_poll.status = PollStatus::Rejected;
            Ok(update_poll)
        },
    )?;
    Ok(Response {
        submessages: vec![],
        messages: vec![],
        attributes: vec![
            attr("action", "present the proposal"),
            attr("proposal_id", poll_id.to_string()),
            attr("proposal_result", "rejected"),
        ],
        data: None,
    })
}

pub fn user_total_weight(deps: &DepsMut, config: &Config, address: &Addr) -> Uint128 {
    let mut weight = Uint128::zero();

    // Ensure sender have some reward tokens
    let msg = LoterraStaking::Holder { address: address.to_string() };

    let loterra_human = deps
        .api
        .addr_humanize(&config.staking_contract_address.clone())
        .unwrap();

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