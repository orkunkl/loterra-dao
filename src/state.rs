use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, CanonicalAddr, Storage, Uint128};
use cosmwasm_storage::{bucket_read, Bucket, ReadonlyBucket};
use cw_storage_plus::{Item, Map};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub count: i32,
    pub owner: Addr,
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
    pub prize_rank: Vec<u8>,
    pub proposal: Proposal,
    pub migration_address: Option<Addr>,
}
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum PollStatus {
    InProgress,
    Passed,
    Rejected,
    RejectedByCreator,
}
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum Proposal {
    LotteryEveryBlockTime,
    HolderFeePercentage,
    DrandWorkerFeePercentage,
    PrizePerRank,
    JackpotRewardPercentage,
    AmountToRegister,
    SecurityMigration,
    DaoFunding,
    StakingContractMigration,
    PollSurvey,
    // test purpose
    NotExist,
}

pub const STATE: Item<State> = Item::new("state");
pub const POLL: Map<&[u8], PollInfoState> = Map::new("poll");
pub const POLL_VOTE: Map<(&[u8], &[u8]), bool> = Map::new("poll_vote");
