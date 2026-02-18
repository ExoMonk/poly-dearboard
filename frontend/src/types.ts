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
