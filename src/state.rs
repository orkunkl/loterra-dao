use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Binary, CanonicalAddr, Uint128};
use cw_storage_plus::{Item, Map};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub admin: Addr,
    pub poll_default_end_height: u64,
    pub staking_contract_address: Addr,
    pub cw20_contract_address: Addr,
}
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub required_collateral: Uint128,
    pub denom: String,
    pub poll_id: u64,
    pub loterry_address: Option<Addr>,
}
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PollInfoState {
    pub creator: CanonicalAddr,
    pub status: PollStatus,
    pub end_height: u64,
    pub start_height: u64,
    pub description: String,
    pub weight_yes_vote: Uint128,
    pub weight_no_vote: Uint128,
    pub yes_vote: u64,
    pub no_vote: u64,
    pub amount: Uint128,
    pub prizes_per_ranks: Vec<u64>,
    pub proposal: Proposal,
    pub recipient: Option<String>,
    pub migration: Option<Migration>,
    pub collateral: Uint128,
    pub applied: bool,
    pub contract_address: Addr,
}
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum PollStatus {
    InProgress,
    Passed,
    Rejected,
    RejectedByCreator,
}
/*
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum Proposal {
    LotteryEveryBlockTime { block_time: Uint128 },
    HolderFeePercentage { percentage: Uint128 },
    DrandWorkerFeePercentage { percentage: Uint128 },
    PrizesPerRanks{ ranks: Vec<u64> },
    JackpotRewardPercentage{ percentage: Uint128 },
    AmountToRegister{ percentage: Uint128 },
    SecurityMigration{ migration: Migration },
    DaoFunding { amount: Uint128 },
    StakingContractMigration {},
    PollSurvey,
    // test purpose
    NotExist,
}
 */
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum Proposal {
    LotteryEveryBlockTime,
    HolderFeePercentage,
    DrandWorkerFeePercentage,
    PrizesPerRanks,
    JackpotRewardPercentage,
    AmountToRegister,
    SecurityMigration,
    DaoFunding,
    StakingContractMigration,
    PollSurvey,
    // test purpose
    NotExist,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Migration {
    pub contract_addr: String,
    pub new_code_id: u64,
    pub msg: Binary,
}

pub const CONFIG: Item<Config> = Item::new("config");
pub const STATE: Item<State> = Item::new("state");
pub const POLL: Map<&[u8], PollInfoState> = Map::new("poll");
pub const POLL_VOTE: Map<(&[u8], &[u8]), bool> = Map::new("poll_vote");
