use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Send some funds")]
    EmptyFunds {},

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
    // Add any other custom errors you like here.
    // Look at https://docs.rs/thiserror/1.0.21/thiserror/ for details.
}
