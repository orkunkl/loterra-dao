use crate::state::{Config, Migration, PollStatus, Proposal, State};
use cosmwasm_std::{Addr, Binary, Decimal, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub code_id: u64,
    pub message: Binary,
    pub label: String,
    pub staking_contract_address: String,
    pub cw20_contract_address: String,
    pub poll_default_end_height: u64,
    pub required_amount: Uint128,
    pub denom: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum ExecuteMsg {
    /// Create proposal
    Poll {
        description: String,
        proposal: Proposal,
        amount: Option<Uint128>,
        prizes_per_ranks: Option<Vec<u8>>,
        recipient: Option<String>,
        migration: Option<Migration>,
    },
    /// Vote proposal
    Vote { poll_id: u64, approve: bool },
    /// Present proposal
    PresentPoll { poll_id: u64 },
    /// Creator reject proposal
    RejectPoll { poll_id: u64 },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum QueryMsg {
    /// Query config
    Config {},
    /// Query state
    State {},
    /// Query poll
    GetPoll { poll_id: u64 },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum LoterraStaking {
    // Get Holder from loterra staking contract
    Holder { address: String },
    State {},
}
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct StakingStateResponse {
    pub global_index: Decimal,
    pub total_balance: Uint128,
    pub prev_reward_balance: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct HolderResponse {
    pub address: String,
    pub balance: Uint128,
    pub index: Decimal,
    pub pending_rewards: Decimal,
}
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct HoldersResponse {
    pub holders: Vec<HolderResponse>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetPollResponse {
    pub creator: Addr,
    pub status: PollStatus,
    pub end_height: u64,
    pub start_height: u64,
    pub description: String,
    pub amount: Uint128,
    pub prizes_per_ranks: Vec<u8>,
    pub recipient: Option<String>,
    pub weight_yes_vote: Uint128,
    pub weight_no_vote: Uint128,
    pub yes_vote: u64,
    pub no_vote: u64,
    pub proposal: Proposal,
    pub migration: Option<Migration>,
    pub collateral: Uint128,
}

// We define a custom struct for each query response
pub type ConfigResponse = Config;
pub type StateResponse = State;
