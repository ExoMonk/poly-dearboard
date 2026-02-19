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
    pub timeframe: Option<String>,
}

#[derive(Deserialize)]
pub struct TradesParams {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub side: Option<String>,
}

// -- Hot Markets --

#[derive(Row, Deserialize)]
pub struct MarketStatsRow {
    pub asset_id: String,
    pub volume: String,
    pub trade_count: u64,
    pub unique_traders: u64,
    pub last_price: String,
    pub last_trade: String,
}

#[derive(Serialize)]
pub struct HotMarket {
    pub token_id: String,
    pub all_token_ids: Vec<String>,
    pub question: String,
    pub outcome: String,
    pub category: String,
    pub volume: String,
    pub trade_count: u64,
    pub unique_traders: u64,
    pub last_price: String,
    pub last_trade: String,
}

#[derive(Serialize)]
pub struct HotMarketsResponse {
    pub markets: Vec<HotMarket>,
}

#[derive(Deserialize)]
pub struct HotMarketsParams {
    pub period: Option<String>,
    pub limit: Option<u32>,
}

// -- Live Feed --

#[derive(Row, Deserialize)]
pub struct RecentTradeRow {
    pub tx_hash: String,
    pub block_timestamp: String,
    pub trader: String,
    pub side: String,
    pub asset_id: String,
    pub amount: String,
    pub price: String,
    pub usdc_amount: String,
}

#[derive(Serialize)]
pub struct FeedTrade {
    pub tx_hash: String,
    pub block_timestamp: String,
    pub trader: String,
    pub side: String,
    pub asset_id: String,
    pub amount: String,
    pub price: String,
    pub usdc_amount: String,
    pub question: String,
    pub outcome: String,
    pub category: String,
}

#[derive(Serialize)]
pub struct LiveFeedResponse {
    pub trades: Vec<FeedTrade>,
}

#[derive(Deserialize)]
pub struct LiveFeedParams {
    pub limit: Option<u32>,
    pub token_id: Option<String>,
}

// -- Trader Positions --

#[derive(Row, Deserialize)]
pub struct PositionRow {
    pub asset_id: String,
    pub side_summary: String,
    pub net_tokens: String,
    pub cost_basis: String,
    pub latest_price: String,
    pub pnl: String,
    pub volume: String,
    pub trade_count: u64,
    pub on_chain_resolved: u8,
}

#[derive(Serialize)]
pub struct OpenPosition {
    pub asset_id: String,
    pub question: String,
    pub outcome: String,
    pub side: String,
    pub net_tokens: String,
    pub cost_basis: String,
    pub latest_price: String,
    pub pnl: String,
    pub volume: String,
    pub trade_count: u64,
}

#[derive(Serialize)]
pub struct PositionsResponse {
    pub open: Vec<OpenPosition>,
    pub closed: Vec<OpenPosition>,
}

// -- PnL Chart --

#[derive(Deserialize)]
pub struct PnlChartParams {
    pub timeframe: Option<String>,
}

/// Per-(bucket, asset) trade summary for mark-to-market PnL computation
#[derive(Row, Deserialize)]
pub struct PnlDailyRow {
    pub date: String,
    pub asset_id: String,
    pub net_token_delta: String,
    pub cash_flow_delta: String,
    pub last_price: String,
}

/// Pre-window portfolio state per asset (for windowed timeframes)
#[derive(Row, Deserialize)]
pub struct PnlInitialStateRow {
    pub asset_id: String,
    pub net_tokens: String,
    pub cash_flow: String,
    pub last_price: String,
}

/// Lightweight read type for resolved_prices lookups
#[derive(Row, Deserialize)]
pub struct ResolvedPriceLookup {
    pub asset_id: String,
    pub resolved_price: String,
}

#[derive(Serialize)]
pub struct PnlChartPoint {
    pub date: String,
    pub pnl: String,
}

#[derive(Serialize)]
pub struct PnlChartResponse {
    pub points: Vec<PnlChartPoint>,
}

// -- Condition Resolution (on-chain) --

#[derive(Row, Deserialize)]
pub struct ConditionResolutionRow {
    pub condition_id: String,
    pub payout_numerators: Vec<String>,
    pub block_number: u64,
}

#[derive(Row, Serialize)]
pub struct ResolvedPriceRow {
    pub asset_id: String,
    pub resolved_price: String,
    pub condition_id: String,
    pub block_number: u64,
}

// -- On-demand market resolve --

#[derive(Deserialize)]
pub struct ResolveParams {
    pub token_ids: String,
}

#[derive(Clone, Serialize)]
pub struct ResolvedMarket {
    pub question: String,
    pub outcome: String,
    pub category: String,
    pub active: bool,
    /// Full-precision token ID from Gamma API (for display/linking)
    pub gamma_token_id: String,
}

// -- Smart Money Signal --

#[derive(Row, Deserialize)]
pub struct SmartMoneyRow {
    pub asset_id: String,
    pub smart_trader_count: u64,
    pub long_count: u64,
    pub short_count: u64,
    pub long_exposure: String,
    pub short_exposure: String,
    pub avg_price: String,
}

#[derive(Serialize, Clone)]
pub struct SmartMoneyMarket {
    pub token_id: String,
    pub question: String,
    pub outcome: String,
    pub smart_trader_count: u64,
    pub long_count: u64,
    pub short_count: u64,
    pub long_exposure: String,
    pub short_exposure: String,
    pub avg_price: String,
}

#[derive(Serialize)]
pub struct SmartMoneyResponse {
    pub markets: Vec<SmartMoneyMarket>,
}
