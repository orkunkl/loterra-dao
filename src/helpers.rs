use crate::error::ContractError;
use crate::state::{PollStatus, State, POLL, STATE};
use cosmwasm_std::{attr, Addr, DepsMut, Response, StdResult, Uint128};

pub fn reject_proposal(deps: DepsMut, poll_id: u64) -> Result<Response, ContractError> {
    POLL.update(
        deps.storage,
        &poll_id.to_be_bytes(),
        |poll| -> StdResult<_> {
            let mut update_poll = poll.unwrap();
            update_poll.status = PollStatus::Rejected;
            Ok(update_poll)
        },
    );
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
/*
pub fn user_total_weight(deps: DepsMut, state: &State, address: &Addr) -> Uint128 {
    let mut weight = Uint128::zero();
    let human_address = deps.api.human_address(&address).unwrap();

    // Ensure sender have some reward tokens
    let msg = QueryMsg::Holder {
        address,
    };
    let loterra_human = deps
        .api
        .human_address(&state.loterra_staking_contract_address.clone())
        .unwrap();
    let res = encode_msg_query(msg, loterra_human).unwrap();
    let loterra_balance = wrapper_msg_loterra_staking(&deps, res).unwrap();

    if !loterra_balance.balance.is_zero() {
        weight += loterra_balance.balance;
    }

    weight
}
*/
