use apollo_cw_asset::{Asset, AssetInfo, AssetInfoUnchecked};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_json_binary, Addr, Binary, CosmosMsg, Deps, DepsMut, Env, Event, MessageInfo, Order,
    Response, StdError, StdResult, Uint128,
};
use cw2::set_contract_version;

use crate::error::ContractError;
use crate::msg::{
    BestPathForPairResponse, CallbackMsg, ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg,
};
use crate::operations::{SwapOperation, SwapOperationsList, SwapOperationsListUnchecked};
use crate::state::{ADMIN, PATHS};

const CONTRACT_NAME: &str = "crates.io:cw-dex-router";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    _msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    ADMIN.set(deps, Some(info.sender))?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::ExecuteSwapOperations {
            operations,
            minimum_receive,
            to,
        } => {
            let operations = operations.check(deps.as_ref())?;
            execute_swap_operations(deps, env, info, operations, minimum_receive, to)
        }
        ExecuteMsg::SetPath {
            offer_asset,
            ask_asset,
            path,
            bidirectional,
        } => {
            let path = path.check(deps.as_ref())?;
            let api = deps.api;
            set_path(
                deps,
                info,
                offer_asset.check(api)?,
                ask_asset.check(api)?,
                path,
                bidirectional,
            )
        }
        ExecuteMsg::Callback(msg) => {
            if info.sender != env.contract.address {
                return Err(ContractError::Unauthorized);
            }
            match msg {
                CallbackMsg::ExecuteSwapOperation { operation, to } => {
                    execute_swap_operation(deps, env, operation, to)
                }
                CallbackMsg::AssertMinimumReceive {
                    asset_info,
                    prev_balance,
                    token_in,
                    minimum_receive,
                    recipient,
                } => assert_minimum_receive(
                    deps,
                    asset_info,
                    prev_balance,
                    token_in,
                    minimum_receive,
                    recipient,
                ),
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn execute_swap_operations(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    operations: SwapOperationsList,
    minimum_receive: Option<Uint128>,
    to: Option<String>,
) -> Result<Response, ContractError> {
    //Validate input or use sender address if None
    let recipient = to.map_or(Ok(info.sender), |x| deps.api.addr_validate(&x))?;

    let target_asset_info = operations.to();
    let offer_asset_info = operations.from();

    // Loop and execute swap operations
    let mut msgs: Vec<CosmosMsg> = operations.into_execute_msgs(&env, recipient.clone())?;

    // Assert min receive
    if let Some(minimum_receive) = minimum_receive {
        let recipient_balance =
            target_asset_info.query_balance(&deps.querier, recipient.clone())?;
        msgs.push(
            CallbackMsg::AssertMinimumReceive {
                asset_info: target_asset_info,
                prev_balance: recipient_balance,
                token_in: Asset::from(info.funds[0].clone()),
                minimum_receive,
                recipient,
            }
            .into_cosmos_msg(&env)?,
        );
    }
    Ok(Response::new().add_messages(msgs))
}

pub fn execute_swap_operation(
    deps: DepsMut,
    env: Env,
    operation: SwapOperation,
    to: Addr,
) -> Result<Response, ContractError> {
    //We use all of the contracts balance.
    let offer_amount = operation
        .offer_asset_info
        .query_balance(&deps.querier, env.contract.address.to_string())?;

    if offer_amount.is_zero() {
        return Ok(Response::default());
    }

    let event = Event::new("apollo/cw-dex-router/callback_execute_swap_operation")
        .add_attribute("operation", format!("{:?}", operation))
        .add_attribute("offer_amount", offer_amount)
        .add_attribute("to", to.to_string());

    Ok(operation
        .to_cosmos_response(deps.as_ref(), &env, offer_amount, None, to)?
        .add_event(event))
}

pub fn assert_minimum_receive(
    deps: DepsMut,
    asset_info: AssetInfo,
    prev_balance: Uint128,
    token_in: Asset,
    minimum_receive: Uint128,
    recipient: Addr,
) -> Result<Response, ContractError> {
    let recipient_balance = asset_info.query_balance(&deps.querier, recipient)?;

    let received_amount = recipient_balance.checked_sub(prev_balance)?;

    if received_amount < minimum_receive {
        return Err(ContractError::FailedMinimumReceive {
            token_in,
            wanted: Asset::new(asset_info.clone(), minimum_receive),
            got: Asset::new(asset_info, received_amount),
        });
    }
    Ok(Response::default())
}

pub fn set_path(
    deps: DepsMut,
    info: MessageInfo,
    offer_asset: AssetInfo,
    ask_asset: AssetInfo,
    path: SwapOperationsList,
    bidirectional: bool,
) -> Result<Response, ContractError> {
    ADMIN.assert_admin(deps.as_ref(), &info.sender)?;

    // Validate the path
    if path.from() != offer_asset || path.to() != ask_asset {
        return Err(ContractError::InvalidSwapOperations {
            operations: path.into(),
            reason: "The path does not match the offer and ask assets".to_string(),
        });
    }

    // check if we have any exisiting items under the offer_asset, ask_asset pair
    // we are looking for the highest ID so we can increment it, this should be under Order::Descending in the first item
    let ps: Result<Vec<(u64, SwapOperationsList)>, StdError> = PATHS
        .prefix((offer_asset.clone().into(), ask_asset.clone().into()))
        .range(deps.storage, None, None, Order::Descending)
        .collect();
    let paths = ps?;
    let last_id = paths.first().map(|(val, _)| val).unwrap_or(&0);

    let new_id = last_id + 1;
    PATHS.save(
        deps.storage,
        ((&offer_asset).into(), (&ask_asset).into(), new_id),
        &path,
    )?;

    // reverse path and store if `bidirectional` is true
    if bidirectional {
        let ps: Result<Vec<(u64, SwapOperationsList)>, StdError> = PATHS
            .prefix((ask_asset.clone().into(), offer_asset.clone().into()))
            .range(deps.storage, None, None, Order::Descending)
            .collect();
        let paths = ps?;
        let last_id = paths.first().map(|(val, _)| val).unwrap_or(&0);

        let new_id = last_id + 1;
        PATHS.save(
            deps.storage,
            (ask_asset.into(), offer_asset.into(), new_id),
            &path.reverse(),
        )?;
    }

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::SimulateSwapOperations {
            offer_amount,
            operations,
        } => to_json_binary(&simulate_swap_operations(deps, offer_amount, operations)?),
        QueryMsg::PathsForPair {
            offer_asset,
            ask_asset,
        } => to_json_binary(&query_paths_for_pair(
            deps,
            offer_asset.check(deps.api)?,
            ask_asset.check(deps.api)?,
        )?),
        QueryMsg::BestPathForPair {
            offer_asset,
            offer_amount,
            ask_asset,
            exclude_paths,
        } => to_json_binary(&query_best_path_for_pair(
            deps,
            offer_amount,
            offer_asset.check(deps.api)?,
            ask_asset.check(deps.api)?,
            exclude_paths,
        )?),
        QueryMsg::SupportedOfferAssets { ask_asset } => {
            to_json_binary(&query_supported_offer_assets(deps, ask_asset)?)
        }
        QueryMsg::SupportedAskAssets { offer_asset } => {
            to_json_binary(&query_supported_ask_assets(deps, offer_asset)?)
        }
    }
}

pub fn simulate_swap_operations(
    deps: Deps,
    mut offer_amount: Uint128,
    operations: SwapOperationsListUnchecked,
) -> Result<Uint128, ContractError> {
    let operations = operations.check(deps)?;

    for operation in operations.into_iter() {
        let offer_asset = Asset::new(operation.offer_asset_info, offer_amount);

        offer_amount = operation
            .pool
            .simulate_swap(deps, offer_asset, operation.ask_asset_info)?;
    }

    Ok(offer_amount)
}

pub fn query_paths_for_pair(
    deps: Deps,
    offer_asset: AssetInfo,
    ask_asset: AssetInfo,
) -> Result<Vec<(u64, SwapOperationsList)>, ContractError> {
    let ps: StdResult<Vec<(u64, SwapOperationsList)>> = PATHS
        .prefix(((&offer_asset).into(), (&ask_asset).into()))
        .range(deps.storage, None, None, Order::Ascending)
        .collect();
    let paths = ps?;
    if paths.is_empty() {
        Err(ContractError::NoPathFound {
            offer: offer_asset.to_string(),
            ask: ask_asset.to_string(),
        })
    } else {
        Ok(paths)
    }
}

pub fn query_best_path_for_pair(
    deps: Deps,
    offer_amount: Uint128,
    offer_asset: AssetInfo,
    ask_asset: AssetInfo,
    exclude_paths: Option<Vec<u64>>,
) -> Result<Option<BestPathForPairResponse>, ContractError> {
    let paths = query_paths_for_pair(deps, offer_asset, ask_asset)?;
    let excluded = exclude_paths.unwrap_or(vec![]);
    let paths: Vec<(u64, SwapOperationsList)> = paths
        .into_iter()
        .filter(|(id, _)| !excluded.contains(id))
        .collect();

    if paths.is_empty() {
        return Err(ContractError::NoPathsToCheck {});
    }

    let swap_paths: Result<Vec<BestPathForPairResponse>, ContractError> = paths
        .into_iter()
        .map(|(_, swaps)| {
            let out = simulate_swap_operations(deps, offer_amount, swaps.clone().into())?;
            Ok(BestPathForPairResponse {
                operations: swaps,
                return_amount: out,
            })
        })
        .collect();

    let best_path = swap_paths?
        .into_iter()
        .max_by(|a, b| a.return_amount.cmp(&b.return_amount));

    Ok(best_path)
}

pub fn query_supported_offer_assets(
    deps: Deps,
    ask_asset: AssetInfoUnchecked,
) -> Result<Vec<AssetInfo>, ContractError> {
    let mut offer_assets: Vec<AssetInfo> = vec![];
    for x in PATHS.range(deps.storage, None, None, Order::Ascending) {
        let ((offer_asset, path_ask_asset, _), _) = x?;
        if path_ask_asset == ask_asset.check(deps.api)? {
            offer_assets.push(offer_asset.into());
        }
    }
    Ok(offer_assets)
}

pub fn query_supported_ask_assets(
    deps: Deps,
    offer_asset: AssetInfoUnchecked,
) -> Result<Vec<AssetInfo>, ContractError> {
    let mut ask_assets: Vec<AssetInfo> = vec![];
    for x in PATHS.range(deps.storage, None, None, Order::Ascending) {
        let ((path_offer_asset, ask_asset, _), _) = x?;
        if path_offer_asset == offer_asset.check(deps.api)? {
            ask_assets.push(ask_asset.into());
        }
    }
    Ok(ask_assets)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    Ok(Response::default())
}
