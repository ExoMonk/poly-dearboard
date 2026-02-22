use clickhouse::Row;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Clone)]
pub struct LeaderboardResponse {
    pub traders: Vec<TraderSummary>,
    pub total: u64,
    pub limit: u32,
    pub offset: u32,
    pub labels: std::collections::HashMap<String, Vec<BehavioralLabel>>,
    pub label_details: std::collections::HashMap<String, LabelDetails>,
}

#[derive(Row, Deserialize, Serialize, Clone)]
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
    /// All token IDs for this market (both sides)
    pub all_token_ids: Vec<String>,
    /// All outcome names (parallel to all_token_ids)
    pub outcomes: Vec<String>,
}

// -- Trader Profile --

#[derive(Row, Deserialize)]
pub struct ProfileAggRow {
    pub avg_position_size: String,
    pub avg_hold_time_hours: f64,
    pub total_positions: u64,
    pub resolved_positions: u64,
}

#[derive(Row, Deserialize)]
pub struct ProfilePositionRow {
    pub asset_id: String,
    pub pnl: String,
    pub total_volume: String,
    pub trade_count: u64,
    pub net_tokens: String,
    pub first_ts: String,
    pub last_ts: String,
    pub resolved_price: String,
    pub on_chain_resolved: u8,
    pub latest_price: String,
    pub buy_usdc: String,
    pub sell_usdc: String,
    pub buy_amount: String,
}

#[derive(Row, Deserialize)]
pub struct BatchPositionRow {
    pub trader: String,
    pub asset_id: String,
    pub pnl: String,
    pub total_volume: String,
    pub trade_count: u64,
    pub net_tokens: String,
    pub first_ts: String,
    pub last_ts: String,
    pub resolved_price: String,
    pub on_chain_resolved: u8,
    pub latest_price: String,
    pub buy_usdc: String,
    pub sell_usdc: String,
    pub buy_amount: String,
}

#[derive(Serialize)]
pub struct PositionHighlight {
    pub asset_id: String,
    pub question: String,
    pub outcome: String,
    pub pnl: String,
}

#[derive(Serialize)]
pub struct CategoryStats {
    pub category: String,
    pub volume: String,
    pub trade_count: u64,
    pub pnl: String,
}

#[derive(Serialize)]
pub struct TraderProfile {
    pub avg_position_size: String,
    pub avg_hold_time_hours: f64,
    pub biggest_win: Option<PositionHighlight>,
    pub biggest_loss: Option<PositionHighlight>,
    pub category_breakdown: Vec<CategoryStats>,
    pub total_positions: u64,
    pub resolved_positions: u64,
    pub labels: Vec<BehavioralLabel>,
    pub label_details: LabelDetails,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BehavioralLabel {
    Sharp,
    Specialist,
    Whale,
    Degen,
    MarketMaker,
    Bot,
    Casual,
    Contrarian,
}

#[derive(Serialize, Clone)]
pub struct LabelDetails {
    pub win_rate: f64,
    pub z_score: f64,
    pub settled_count: u64,
    pub dominant_category: String,
    pub dominant_category_pct: f64,
    pub category_win_rate: f64,
    pub total_volume: String,
    pub avg_position_size_usd: String,
    pub unique_markets: u64,
    pub total_trade_count: u64,
    pub active_span_days: f64,
    pub buy_sell_ratio: f64,
    pub trades_per_market: f64,
    pub contrarian_trades: u64,
    pub contrarian_correct: u64,
    pub contrarian_rate: f64,
}

// -- Smart Money Signal --

#[derive(Deserialize)]
pub struct SmartMoneyParams {
    pub top: Option<u32>,
    pub timeframe: Option<String>,
}

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
    pub top: u32,
}

// -- Trader Lists --

#[derive(Serialize)]
pub struct TraderList {
    pub id: String,
    pub name: String,
    pub member_count: u32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize)]
pub struct TraderListDetail {
    pub id: String,
    pub name: String,
    pub members: Vec<TraderListMember>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize)]
pub struct TraderListMember {
    pub address: String,
    pub label: Option<String>,
    pub added_at: String,
}

#[derive(Deserialize)]
pub struct CreateListRequest {
    pub name: String,
}

#[derive(Deserialize)]
pub struct RenameListRequest {
    pub name: String,
}

#[derive(Deserialize)]
pub struct AddMembersRequest {
    pub addresses: Vec<String>,
    pub labels: Option<Vec<Option<String>>>,
}

#[derive(Deserialize)]
pub struct RemoveMembersRequest {
    pub addresses: Vec<String>,
}

// -- PolyLab Backtest --

#[derive(Deserialize)]
pub struct BacktestRequest {
    pub top_n: Option<u32>,
    pub list_id: Option<String>,
    pub timeframe: String,
    pub initial_capital: Option<f64>,
    pub copy_pct: Option<f64>,
}

#[derive(Row, Deserialize)]
pub struct PnlDailyTraderRow {
    pub trader: String,
    pub date: String,
    pub asset_id: String,
    pub net_token_delta: String,
    pub cash_flow_delta: String,
    pub last_price: String,
}

#[derive(Row, Deserialize)]
pub struct PnlInitialStateTraderRow {
    pub trader: String,
    pub asset_id: String,
    pub net_tokens: String,
    pub cash_flow: String,
    pub last_price: String,
}

#[derive(Row, Deserialize)]
pub struct TraderScaleRow {
    pub address: String,
    pub avg_position_size: String,
    pub market_count: u64,
}

#[derive(Serialize)]
pub struct PortfolioPoint {
    pub date: String,
    pub value: String,
    pub pnl: String,
    pub pnl_pct: String,
}

#[derive(Serialize)]
pub struct BacktestConfig {
    pub initial_capital: f64,
    pub copy_pct: f64,
    pub top_n: u32,
    pub timeframe: String,
    pub per_trader_budget: f64,
}

#[derive(Serialize)]
pub struct BacktestResponse {
    pub portfolio_curve: Vec<PortfolioPoint>,
    pub pnl_curve: Vec<PnlChartPoint>,
    pub summary: BacktestSummary,
    pub traders: Vec<BacktestTrader>,
    pub config: BacktestConfig,
}

#[derive(Serialize)]
pub struct BacktestSummary {
    pub total_pnl: String,
    pub total_return_pct: f64,
    pub win_rate: f64,
    pub max_drawdown: String,
    pub max_drawdown_pct: f64,
    pub positions_count: u64,
    pub traders_count: u32,
    pub initial_capital: f64,
    pub final_value: f64,
}

#[derive(Serialize)]
pub struct BacktestTrader {
    pub address: String,
    pub rank: u32,
    pub pnl: String,
    pub scaled_pnl: String,
    pub markets_traded: u64,
    pub contribution_pct: f64,
    pub scale_factor: f64,
}

// -- Copy Portfolio --

#[derive(Deserialize)]
pub struct CopyPortfolioParams {
    pub top: Option<u32>,
    pub list_id: Option<String>,
}

#[derive(Row, Deserialize)]
pub struct CopyPortfolioRow {
    pub trader: String,
    pub asset_id: String,
    pub net_tokens: String,
    pub avg_entry: String,
    pub latest_price: String,
    pub exposure: String,
    pub pnl: String,
}

#[derive(Serialize)]
pub struct CopyPortfolioPosition {
    pub token_id: String,
    pub question: String,
    pub outcome: String,
    pub convergence: u32,
    pub long_count: u32,
    pub short_count: u32,
    pub total_exposure: String,
    pub avg_entry: String,
    pub latest_price: String,
    pub total_pnl: String,
}

#[derive(Serialize)]
pub struct CopyPortfolioSummary {
    pub total_positions: u32,
    pub unique_markets: u32,
    pub total_exposure: String,
    pub total_pnl: String,
    pub top_n: u32,
}

#[derive(Serialize)]
pub struct CopyPortfolioResponse {
    pub positions: Vec<CopyPortfolioPosition>,
    pub summary: CopyPortfolioSummary,
}

// -- Trading Wallet --

#[derive(Serialize)]
pub struct TradingWalletInfo {
    pub id: String,
    pub address: String,
    pub proxy_address: Option<String>,
    pub status: String,
    pub has_clob_credentials: bool,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct WalletGenerateResponse {
    pub id: String,
    pub address: String,
    pub private_key: String,
    pub proxy_address: String,
}

#[derive(Deserialize)]
pub struct ImportWalletRequest {
    pub private_key: String,
}

#[derive(Serialize)]
pub struct ImportWalletResponse {
    pub id: String,
    pub address: String,
    pub proxy_address: String,
}

#[derive(Serialize)]
pub struct DeriveCredentialsResponse {
    pub success: bool,
    pub wallet_id: String,
    pub api_key: String,
}

// -- Market Metadata (persisted from Gamma API cache to ClickHouse) --

#[derive(clickhouse::Row, Serialize, Deserialize)]
pub struct MarketMetadataRow {
    pub asset_id: String,
    pub question: String,
    pub outcome: String,
    pub category: String,
    pub condition_id: String,
    pub gamma_token_id: String,
    pub outcome_index: u8,
    pub active: u8,
    pub all_token_ids: Vec<String>,
    pub outcomes: Vec<String>,
    pub updated_at: u32,
}
