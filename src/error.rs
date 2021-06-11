use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Do not send funds")]
    DoNotSendFunds {},

    #[error("Poll is expired")]
    PollExpired {},

    #[error("Poll vote closed")]
    PollClosed {},

    #[error("Already voted")]
    AlreadyVoted {},

    #[error("Only stakers can vote")]
    OnlyStakers {},
    // Add any other custom errors you like here.
    // Look at https://docs.rs/thiserror/1.0.21/thiserror/ for details.
}
