use cosmwasm_schema::cw_serde;
use cosmwasm_std::Addr;
use cw_controllers::Admin;
use cw_storage_plus::{Item, Map};
use osmosis_std::types::osmosis::poolmanager::v1beta1::SwapAmountInRoute;

#[cw_serde]
pub struct RecipientInfo {
    pub address: Addr,
    pub denom: String,
}

/// As an MVP we hardcode paths for each tuple of assets (offer, ask).
/// In a future version we want to find the path that produces the highest
/// number of ask assets, but this will take some time to implement.
/// To support multiple paths between the same asset, we add an id field we increment per asset of the same
/// path
pub const PATHS: Map<(String, String, u64), Vec<SwapAmountInRoute>> = Map::new("paths");
pub const ADMIN: Admin = Admin::new("admin");
pub const RECIPIENT_INFO: Item<RecipientInfo> = Item::new("recipient");
