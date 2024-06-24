use std::vec;

use apollo_cw_asset::{Asset, AssetInfo};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    to_json_binary, Addr, Api, Coin, CosmosMsg, MessageInfo, QuerierWrapper, QueryRequest,
    StdError, StdResult, Uint128, WasmMsg, WasmQuery,
};

use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::operations::SwapOperationsList;

#[cw_serde]
pub struct CwDexRouterBase<T>(pub T);

pub type CwDexRouterUnchecked = CwDexRouterBase<String>;
pub type CwDexRouter = CwDexRouterBase<Addr>;

impl From<CwDexRouter> for CwDexRouterUnchecked {
    fn from(x: CwDexRouter) -> Self {
        CwDexRouterBase(x.0.to_string())
    }
}

impl<T> From<T> for CwDexRouterBase<T> {
    fn from(x: T) -> Self {
        CwDexRouterBase(x)
    }
}

impl CwDexRouterUnchecked {
    pub const fn new(addr: String) -> Self {
        CwDexRouterBase(addr)
    }

    pub fn check(&self, api: &dyn Api) -> StdResult<CwDexRouter> {
        Ok(CwDexRouter::new(&api.addr_validate(&self.0)?))
    }

    pub fn instantiate(
        code_id: u64,
        admin: Option<String>,
        label: Option<String>,
    ) -> StdResult<CosmosMsg> {
        Ok(CosmosMsg::Wasm(WasmMsg::Instantiate {
            code_id,
            admin,
            msg: to_json_binary(&InstantiateMsg {})?,
            funds: vec![],
            label: label.unwrap_or_else(|| "cw-dex-router".to_string()),
        }))
    }
}

impl CwDexRouter {
    pub fn new(contract_addr: &Addr) -> Self {
        Self(contract_addr.clone())
    }

    pub fn addr(&self) -> Addr {
        self.0.clone()
    }

    pub fn call<T: Into<ExecuteMsg>>(&self, msg: T, funds: Vec<Coin>) -> StdResult<CosmosMsg> {
        let msg = to_json_binary(&msg.into())?;
        Ok(WasmMsg::Execute {
            contract_addr: self.addr().into(),
            msg,
            funds,
        }
        .into())
    }

    pub fn execute_swap_operations_msg(
        &self,
        operations: &SwapOperationsList,
        minimum_receive: Option<Uint128>,
        to: Option<String>,
        funds: Vec<Coin>,
    ) -> StdResult<CosmosMsg> {
        self.call(
            ExecuteMsg::ExecuteSwapOperations {
                operations: operations.into(),
                minimum_receive,
                to,
            },
            funds,
        )
    }

    pub fn set_path_msg(
        &self,
        offer_asset: AssetInfo,
        ask_asset: AssetInfo,
        path: &SwapOperationsList,
        bidirectional: bool,
    ) -> StdResult<CosmosMsg> {
        self.call(
            ExecuteMsg::SetPath {
                offer_asset: offer_asset.into(),
                ask_asset: ask_asset.into(),
                path: path.into(),
                bidirectional,
            },
            vec![],
        )
    }

    pub fn simulate_swap_operations(
        &self,
        querier: &QuerierWrapper,
        offer_amount: Uint128,
        operations: &SwapOperationsList,
    ) -> StdResult<Uint128> {
        querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: self.0.to_string(),
            msg: to_json_binary(&QueryMsg::SimulateSwapOperations {
                offer_amount,
                operations: operations.into(),
            })?,
        }))
    }

    pub fn query_path_for_pair(
        &self,
        querier: &QuerierWrapper,
        offer_asset: &AssetInfo,
        ask_asset: &AssetInfo,
    ) -> StdResult<SwapOperationsList> {
        querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: self.0.to_string(),
            msg: to_json_binary(&QueryMsg::PathsForPair {
                offer_asset: offer_asset.to_owned().into(),
                ask_asset: ask_asset.to_owned().into(),
            })?,
        }))
    }

    pub fn query_supported_offer_assets(
        &self,
        querier: &QuerierWrapper,
        ask_asset: &AssetInfo,
    ) -> StdResult<Vec<AssetInfo>> {
        querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: self.0.to_string(),
            msg: to_json_binary(&QueryMsg::SupportedOfferAssets {
                ask_asset: ask_asset.to_owned().into(),
            })?,
        }))
    }

    pub fn query_supported_ask_assets(
        &self,
        querier: &QuerierWrapper,
        offer_asset: &AssetInfo,
    ) -> StdResult<Vec<AssetInfo>> {
        querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: self.0.to_string(),
            msg: to_json_binary(&QueryMsg::SupportedAskAssets {
                offer_asset: offer_asset.to_owned().into(),
            })?,
        }))
    }
}

/// Assert that a specific native token in the form of an `Asset` was sent to
/// the contract.
pub fn assert_native_token_received(info: &MessageInfo, asset: &Asset) -> StdResult<()> {
    let coin: Coin = asset.try_into()?;

    if !info.funds.contains(&coin) {
        return Err(StdError::generic_err(format!(
            "Assert native token receive failed for asset: {}",
            asset
        )));
    }
    Ok(())
}
