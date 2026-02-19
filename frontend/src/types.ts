export interface TraderSummary {
  address: string;
  total_volume: string;
  trade_count: number;
  markets_traded: number;
  realized_pnl: string;
  total_fees: string;
  first_trade: string;
  last_trade: string;
}

export interface LeaderboardResponse {
  traders: TraderSummary[];
  total: number;
  limit: number;
  offset: number;
}

export interface TradeRecord {
  tx_hash: string;
  block_number: number;
  block_timestamp: string;
  exchange: string;
  side: string;
  asset_id: string;
  amount: string;
  price: string;
  usdc_amount: string;
  fee: string;
}

export interface TradesResponse {
  trades: TradeRecord[];
  total: number;
  limit: number;
  offset: number;
}

export interface HealthResponse {
  status: string;
  trade_count: number;
  trader_count: number;
  latest_block: number;
}

export type SortColumn = "realized_pnl" | "total_volume" | "trade_count";
export type SortOrder = "asc" | "desc";
export type Timeframe = "1h" | "24h" | "all";
export type PnlTimeframe = "24h" | "7d" | "30d" | "all";

export interface HotMarket {
  token_id: string;
  all_token_ids: string[];
  question: string;
  outcome: string;
  category: string;
  volume: string;
  trade_count: number;
  unique_traders: number;
  last_price: string;
  last_trade: string;
}

export interface HotMarketsResponse {
  markets: HotMarket[];
}

export interface FeedTrade {
  tx_hash: string;
  block_timestamp: string;
  trader: string;
  side: string;
  asset_id: string;
  amount: string;
  price: string;
  usdc_amount: string;
  question: string;
  outcome: string;
  category: string;
}

export interface LiveFeedResponse {
  trades: FeedTrade[];
}

export interface OpenPosition {
  asset_id: string;
  question: string;
  outcome: string;
  side: string;
  net_tokens: string;
  cost_basis: string;
  latest_price: string;
  pnl: string;
  volume: string;
  trade_count: number;
}

export interface PositionsResponse {
  open: OpenPosition[];
  closed: OpenPosition[];
}

export interface PnlChartPoint {
  date: string;
  pnl: string;
}

export interface PnlChartResponse {
  points: PnlChartPoint[];
}

export interface ResolvedMarket {
  question: string;
  outcome: string;
  category: string;
  active: boolean;
  gamma_token_id: string;
}

// Smart Money Signal

export interface SmartMoneyMarket {
  token_id: string;
  question: string;
  outcome: string;
  smart_trader_count: number;
  long_count: number;
  short_count: number;
  long_exposure: string;
  short_exposure: string;
  avg_price: string;
}

export interface SmartMoneyResponse {
  markets: SmartMoneyMarket[];
}

// Alerts (WebSocket)

export interface WhaleTradeAlert {
  kind: "WhaleTrade";
  timestamp: string;
  exchange: string;
  side: string;
  trader: string;
  asset_id: string;
  usdc_amount: string;
  token_amount: string;
  tx_hash: string;
  block_number: number;
  question?: string;
  outcome?: string;
}

export interface MarketResolutionAlert {
  kind: "MarketResolution";
  timestamp: string;
  condition_id: string;
  oracle: string;
  question_id: string;
  payout_numerators: string[];
  tx_hash: string;
  block_number: number;
  question?: string;
  winning_outcome?: string;
  outcomes: string[];
  token_id?: string;
}

export interface FailedSettlementAlert {
  kind: "FailedSettlement";
  tx_hash: string;
  block_number: number;
  timestamp: string;
  from_address: string;
  to_contract: string;
  function_name: string;
  gas_used: string;
}

export type Alert = WhaleTradeAlert | MarketResolutionAlert | FailedSettlementAlert;

// Polymarket WebSocket (live market data)

export interface PricePoint {
  timestamp: number;
  yesPrice: number;
  noPrice: number;
}

export interface TradePoint {
  timestamp: number;
  price: number;
  side: "buy" | "sell";
  size: number;
}

export type MarketWsStatus = "connecting" | "connected" | "disconnected";

export interface BidAsk {
  bestBid: number | null;
  bestAsk: number | null;
  spread: number | null;
}
