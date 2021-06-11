use crate::state::{PollStatus, Proposal};
use cosmwasm_std::{Addr, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::ops::Add;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub count: i32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Create proposal
    Poll {
        description: String,
        proposal: Proposal,
        amount: Option<Uint128>,
        prize_per_rank: Option<Vec<u8>>,
        recipient: Option<Addr>,
    },
    /// Vote proposal
    Vote { poll_id: u64, approve: bool },
    /// Present proposal
    PresentPoll { poll_id: u64 },
    /// Creator reject proposal
    RejectPoll { poll_id: u64 },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Get poll
    GetPoll { poll_id: u64 },
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct CountResponse {
    pub count: i32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetPollResponse {
    pub creator: Addr,
    pub status: PollStatus,
    pub end_height: u64,
    pub start_height: u64,
    pub description: String,
    pub amount: Uint128,
    pub prize_per_rank: Vec<u8>,
    pub migration_address: Option<Addr>,
    pub weight_yes_vote: Uint128,
    pub weight_no_vote: Uint128,
    pub yes_vote: u64,
    pub no_vote: u64,
    pub proposal: Proposal,
}
