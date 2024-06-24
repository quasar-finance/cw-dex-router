use cosmwasm_std::{OverflowError, StdError};
use cw_asset::AssetError;
use cw_controllers::AdminError;
use osmosis_std::types::osmosis::poolmanager::v1beta1::SwapAmountInRoute;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    Overflow(#[from] OverflowError),

    #[error("{0}")]
    AdminError(#[from] AdminError),

    #[error("{0}")]
    Asset(#[from] AssetError),

    #[error("Incorrect amount of native token sent. You don't need to pass in offer_amount if using native tokens.")]
    IncorrectNativeAmountSent,

    #[error("Unsupported asset type. Only native and cw20 tokens are supported.")]
    UnsupportedAssetType,

    #[error("No swap operations provided")]
    MustProvideOperations,

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Invalid swap operations: {operations:?} {reason}")]
    InvalidSwapOperations {
        operations: Vec<SwapAmountInRoute>,
        reason: String,
    },

    #[error("Paths to check is empty, excluded paths excludes all valid paths")]
    NoPathsToCheck,

    #[error("No path found for assets {offer:?} -> {ask:?}")]
    NoPathFound { offer: String, ask: String },
}

impl From<ContractError> for StdError {
    fn from(x: ContractError) -> Self {
        Self::generic_err(x.to_string())
    }
}
