use clickhouse::Row;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct LeaderboardResponse {
    pub traders: Vec<TraderSummary>,
    pub total: u64,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Row, Deserialize, Serialize)]
pub struct TraderSummary {
    pub address: String,
    pub total_volume: String,
    pub trade_count: u64,
    pub markets_traded: u64,
    pub realized_pnl: String,
    pub total_fees: String,
    pub first_trade: String,
    pub last_trade: String,
}

#[derive(Serialize)]
pub struct TradesResponse {
    pub trades: Vec<TradeRecord>,
    pub total: u64,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Row, Deserialize, Serialize)]
pub struct TradeRecord {
    pub tx_hash: String,
    pub block_number: u64,
    pub block_timestamp: String,
    pub exchange: String,
    pub side: String,
    pub asset_id: String,
    pub amount: String,
    pub price: String,
    pub usdc_amount: String,
    pub fee: String,
}

#[derive(Row, Deserialize, Serialize)]
pub struct HealthStats {
    pub trade_count: u64,
    pub trader_count: u64,
    pub latest_block: u64,
}

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub trade_count: u64,
    pub trader_count: u64,
    pub latest_block: u64,
}

#[derive(Deserialize)]
pub struct LeaderboardParams {
    pub sort: Option<String>,
    pub order: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Deserialize)]
pub struct TradesParams {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub side: Option<String>,
}
