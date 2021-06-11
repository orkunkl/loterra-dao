use cosmwasm_std::{
    attr, entry_point, to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult,
    Uint128,
};

use crate::error::ContractError;
use crate::msg::ExecuteMsg::Poll;
use crate::msg::{CountResponse, ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::state::{PollStatus, State, POLL, POLL_VOTE, STATE};
//use crate::helpers::user_total_weight;
use std::ops::Add;

// Note, you can use StdResult in some functions where you do not
// make use of the custom errors
#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let state = State {
        count: msg.count,
        owner: info.sender,
    };
    STATE.save(deps.storage, &state)?;
    Ok(Response::default())
}

// And declare a custom Error variant for the ones where you will want to make use of it
#[entry_point]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Vote { poll_id, approve } => try_vote(deps, info, env, poll_id, approve),
        _ => Ok(Response {
            submessages: vec![],
            messages: vec![],
            attributes: vec![],
            data: None,
        }),
    }
}
pub fn try_vote(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    poll_id: u64,
    approve: bool,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
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
        (&sender, &poll_id.to_be_bytes()),
        |exist| match exist {
            None => Ok(approve),
            Some(_) => Err(ContractError::AlreadyVoted {}),
        },
    )?;

    // Get the sender weight
    //let weight = user_total_weight(deps.clone(), &state, info.sender);
    let weight = Uint128(200);
    // Only stakers can vote
    if weight.is_zero() {
        return Err(ContractError::OnlyStakers {});
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

#[entry_point]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetPoll { .. } => to_binary(&query_count(deps)?),
    }
}

fn query_count(deps: Deps) -> StdResult<CountResponse> {
    let state = STATE.load(deps.storage)?;
    Ok(CountResponse { count: state.count })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{coins, from_binary};

    fn default_init(deps: DepsMut) {
        let msg = InstantiateMsg { count: 17 };
        let info = mock_info("creator", &coins(1000, "earth"));
        // we can just call .unwrap() to assert this was a success
        let res = instantiate(deps, mock_env(), info, msg).unwrap();
    }
    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies(&[]);

        let msg = InstantiateMsg { count: 17 };
        let info = mock_info("creator", &coins(1000, "earth"));

        // we can just call .unwrap() to assert this was a success
        let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(0, res.messages.len());

        // it worked, let's query the state
        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetPoll { poll_id: 0 }).unwrap();
        let value: CountResponse = from_binary(&res).unwrap();
        assert_eq!(17, value.count);
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
    }
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
