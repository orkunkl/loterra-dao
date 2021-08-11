use cosmwasm_std::{StdError, Uint128};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Send some funds")]
    EmptyFunds {},

    #[error("Send {0} as collateral in order to create a proposal")]
    RequiredCollateral(Uint128),

    #[error("Multiple denoms not allowed")]
    MultipleDenoms {},

    #[error("Wrong denom")]
    WrongDenom {},

    #[error("Amount is required")]
    EmptyAmount {},

    #[error("Retry redeem after block height `{0}`")]
    RetryRedeemLater(u64),

    #[error("Do not send funds")]
    DoNotSendFunds {},

    #[error("Not enough funds")]
    NotEnoughFunds {},

    #[error("Wrong description length {0}, minimum {1} maximum {2}")]
    WrongDescLength(usize, u64, u64),

    #[error("Amount between 0 to {0}")]
    MaxReward(u8),

    #[error("Invalid amount")]
    InvalidAmount(),

    #[error("Invalid block time")]
    InvalidBlockTime(),

    #[error("Invalid rank")]
    InvalidRank(),

    #[error("Numbers between 0 to 1000")]
    InvalidNumber(),

    #[error("Numbers total sum need to be equal to 1000")]
    InvalidNumberSum(),

    #[error("Migration is required")]
    InvalidMigration(),

    #[error("Migration address is required")]
    NoMigrationAddress(),

    #[error("Unknown proposal type")]
    UnknownProposalType(),

    #[error("Poll expired")]
    PollExpired {},

    #[error("Poll closed")]
    PollClosed {},

    #[error("Poll not found")]
    PollNotFound {},

    #[error("Only stakers can vote")]
    OnlyStakersVote {},

    #[error("Proposal expired")]
    ProposalExpired {},

    #[error("Proposal inprogress")]
    ProposalInProgress {},

    #[error("Already voted")]
    AlreadyVoted {},
}
