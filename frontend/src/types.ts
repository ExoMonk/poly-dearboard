import type { ReactNode } from "react";

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
  labels: Record<string, BehavioralLabel[]>;
  label_details: Record<string, LabelDetails>;
  readiness: Record<string, TraderReadiness>;
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
  positions: OpenPosition[];
  total: number;
  open_count: number;
  closed_count: number;
}

export interface PnlChartPoint {
  date: string;
  pnl: string;
}

export interface PnlChartResponse {
  points: PnlChartPoint[];
}

export interface PnlBar {
  date: string;
  realized: string;
  unrealized?: string;
}

export interface PnlBarChartResponse {
  bars: PnlBar[];
}

export interface ResolvedMarket {
  question: string;
  outcome: string;
  category: string;
  active: boolean;
  gamma_token_id: string;
  all_token_ids: string[];
  outcomes: string[];
  condition_id?: string;
}

// Smart Money Signal

export interface SmartMoneyMarket {
  token_id: string;
  question: string;
  outcome: string;
  counter_outcome: string;
  smart_trader_count: number;
  long_count: number;
  short_count: number;
  long_exposure: string;
  short_exposure: string;
  avg_price: string;
}

export interface SmartMoneyResponse {
  markets: SmartMoneyMarket[];
  top: number;
}

// Trader Profile

export type BehavioralLabel =
  | "sharp"
  | "specialist"
  | "whale"
  | "degen"
  | "market_maker"
  | "bot"
  | "casual"
  | "contrarian";

export interface PositionHighlight {
  asset_id: string;
  question: string;
  outcome: string;
  pnl: string;
}

export interface CategoryStats {
  category: string;
  volume: string;
  trade_count: number;
  pnl: string;
}

export interface LabelDetails {
  win_rate: number;
  win_rate_confidence: "low" | "medium" | "high";
  z_score: number;
  settled_count: number;
  dominant_category: string;
  dominant_category_pct: number;
  category_win_rate: number;
  total_volume: string;
  avg_position_size_usd: string;
  unique_markets: number;
  total_trade_count: number;
  active_span_days: number;
  buy_sell_ratio: number;
  trades_per_market: number;
  contrarian_trades: number;
  contrarian_correct: number;
  contrarian_rate: number;
}

export interface TraderProfile {
  avg_position_size: string;
  avg_hold_time_hours: number;
  biggest_win: PositionHighlight | null;
  biggest_loss: PositionHighlight | null;
  category_breakdown: CategoryStats[];
  total_positions: number;
  resolved_positions: number;
  labels: BehavioralLabel[];
  label_details: LabelDetails;
  readiness?: TraderReadiness;
}

// Copytrade Readiness (spec 28)

export type DiscoveryCategory =
  | "momentum"
  | "consistent"
  | "high_conviction"
  | "fast_mover"
  | "contrarian"
  | "volume_maker";

export type ReadinessBucket = "low" | "medium" | "high";

export interface ReadinessMetrics {
  settled_count: number;
  unique_markets: number;
  active_span_days: number;
  near_resolved_volume_ratio?: number;
  avg_hold_hours?: number;
  roi?: number;
  adjusted_win_rate?: number;
}

export interface TraderReadiness {
  score: number;
  bucket: ReadinessBucket;
  score_version: string;
  computed_at: string;
  timeframe: string;
  categories: DiscoveryCategory[];
  reason_codes: string[];
  reasons: string[];
  confidence: ReadinessBucket;
  metrics: ReadinessMetrics;
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

// PolyLab Backtest

export type BacktestTimeframe = "7d" | "30d" | "all";

export interface PortfolioPoint {
  date: string;
  value: string;
  pnl: string;
  pnl_pct: string;
}

export interface BacktestConfig {
  initial_capital: number;
  copy_pct: number;
  top_n: number;
  timeframe: string;
  per_trader_budget: number;
}

export interface BacktestSummary {
  total_pnl: string;
  total_return_pct: number;
  win_rate: number;
  max_drawdown: string;
  max_drawdown_pct: number;
  positions_count: number;
  traders_count: number;
  initial_capital: number;
  final_value: number;
}

export interface BacktestTrader {
  address: string;
  rank: number;
  pnl: string;
  scaled_pnl: string;
  markets_traded: number;
  contribution_pct: number;
  scale_factor: number;
}

export interface BacktestResponse {
  portfolio_curve: PortfolioPoint[];
  pnl_curve: PnlChartPoint[];
  summary: BacktestSummary;
  traders: BacktestTrader[];
  config: BacktestConfig;
}

// Copy Portfolio

export interface CopyPortfolioPosition {
  token_id: string;
  question: string;
  outcome: string;
  convergence: number;
  long_count: number;
  short_count: number;
  total_exposure: string;
  avg_entry: string;
  latest_price: string;
  total_pnl: string;
}

export interface CopyPortfolioSummary {
  total_positions: number;
  unique_markets: number;
  total_exposure: string;
  total_pnl: string;
  top_n: number;
}

export interface CopyPortfolioResponse {
  positions: CopyPortfolioPosition[];
  summary: CopyPortfolioSummary;
}

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

// Trader Lists

export interface TraderList {
  id: string;
  name: string;
  member_count: number;
  created_at: string;
  updated_at: string;
}

export interface TraderListMember {
  address: string;
  label?: string;
  added_at: string;
}

export interface TraderListDetail {
  id: string;
  name: string;
  members: TraderListMember[];
  created_at: string;
  updated_at: string;
}

// Signal Feed (WebSocket)

export interface SignalTrade {
  kind: "Trade";
  tx_hash: string;
  block_timestamp: string;
  trader: string;
  side: string;
  asset_id: string;
  amount: string;
  price: string;
  usdc_amount: string;
  question?: string;
  outcome?: string;
}

export interface ConvergenceAlert {
  kind: "Convergence";
  asset_id: string;
  traders: string[];
  side: string;
  window_seconds: number;
  question?: string;
  outcome?: string;
}

export interface LagMessage {
  kind: "Lag";
  dropped: number;
}

export type SignalMessage = SignalTrade | ConvergenceAlert | LagMessage;

// Trading Wallet (spec 13)

export interface TradingWalletInfo {
  id: string;
  address: string;
  proxy_address: string | null;
  status: "created" | "credentialed" | "active" | "disabled";
  has_clob_credentials: boolean;
  created_at: string;
}

export interface WalletGenerateResponse {
  id: string;
  address: string;
  private_key: string;
  proxy_address: string;
}

export interface ImportWalletResponse {
  id: string;
  address: string;
  proxy_address: string;
}

export interface DeriveCredentialsResponse {
  success: boolean;
  wallet_id: string;
  api_key: string;
}

// Wallet Funding (spec 14)

export interface WalletBalance {
  usdc_balance: string;
  usdc_raw: string;
  ctf_exchange_approved: boolean;
  neg_risk_exchange_approved: boolean;
  pol_balance: string;
  needs_gas: boolean;
  safe_deployed: boolean;
  last_checked_secs_ago: number | null;
}

export interface ApprovalResult {
  tx_hash: string | null;
  already_approved: boolean;
}

export interface DepositAddresses {
  evm: string;
  svm: string;
  btc: string;
  note: string | null;
}

export interface DepositStatus {
  pending: PendingDeposit[];
}

export interface PendingDeposit {
  from_chain: string;
  token: string;
  amount: string;
  status: string;
  tx_hash: string | null;
}

// Terminal Shell (spec 12, extended in spec 17)
export type LogSource = "wallet" | "copytrade" | "alert";

export interface LogEntry {
  id: string;
  timestamp: number;
  level: "info" | "warn" | "error" | "success";
  source: LogSource;
  message: string;
  meta?: Record<string, string>;
}

export type TerminalTab = "wallet" | "sessions" | "logs" | "orders" | "feed" | "alerts";

export type LiveFeedMode = "signals" | "public";
export type TerminalHeight = "collapsed" | "half" | "full";
export type WalletStatus = "none" | "setup" | "funded" | "active";

export interface PaletteCommand {
  id: string;
  label: string;
  section: "Navigation" | "Terminal" | "Session" | "Quick Actions";
  icon?: ReactNode;
  shortcut?: string;
  keywords?: string[];
  action: () => void;
  available?: () => boolean;
}

// Quick-action prefill contract (spec 25)
export interface CreateSessionPrefill {
  sourceSurface: "alerts" | "activity" | "market_live_feed" | "feed";
  sourceKind?:
    | "whale_trade"
    | "resolution"
    | "failed_settlement"
    | "hot_market"
    | "market_trade"
    | "signal_trade"
    | "public_trade";
  traderAddress?: string;
  tokenId?: string;
  question?: string;
  outcome?: string;
  defaults?: {
    simulationMode?: "simulate" | "live";
    maxPositionUsd?: number;
    copySizePercent?: number;
    minSourceTradeUsd?: number;
  };
}

// Copy-Trade Engine (spec 15)
export type SessionStatus = "running" | "paused" | "stopped";
export type CopyOrderType = "FOK" | "GTC";
export type OrderStatus = "pending" | "submitted" | "filled" | "partial" | "failed" | "canceled" | "simulated";

export interface CreateSessionRequest {
  wallet_id?: string;
  list_id?: string;
  top_n?: number;
  copy_pct: number;
  max_position_usdc: number;
  max_slippage_bps: number;
  order_type: CopyOrderType;
  initial_capital: number;
  simulate: boolean;
  max_loss_pct?: number;
  min_source_usdc?: number;
  utilization_cap?: number;
  max_open_positions?: number;
  take_profit_pct?: number;
  stop_loss_pct?: number;
  mirror_close?: boolean;
  health_interval_secs?: number;
  max_source_price?: number;
  min_source_price?: number;
}

export interface CopyTradeSession {
  id: string;
  wallet_id: string | null;
  list_id: string | null;
  top_n: number | null;
  copy_pct: number;
  max_position_usdc: number;
  max_slippage_bps: number;
  order_type: CopyOrderType;
  initial_capital: number;
  remaining_capital: number;
  positions_value: number;
  simulate: boolean;
  max_loss_pct: number | null;
  min_source_usdc: number;
  utilization_cap: number;
  max_open_positions: number;
  take_profit_pct: number | null;
  stop_loss_pct: number | null;
  mirror_close: boolean;
  health_interval_secs: number;
  max_source_price: number;
  min_source_price: number;
  status: SessionStatus;
  created_at: string;
  updated_at: string;
}

export interface CopyTradeOrder {
  id: string;
  session_id: string;
  source_tx_hash: string;
  source_trader: string;
  clob_order_id: string | null;
  asset_id: string;
  side: string;
  price: number;
  source_price: number;
  size_usdc: number;
  size_shares: number | null;
  status: OrderStatus;
  error_message: string | null;
  fill_price: number | null;
  slippage_bps: number | null;
  tx_hash: string | null;
  created_at: string;
  updated_at: string;
}

export interface CopyTradeOrderSummary {
  id: string;
  asset_id: string;
  side: string;
  size_usdc: number;
  price: number;
  source_trader: string;
  simulate: boolean;
}

export type CopyTradeUpdate =
  | { kind: "OrderPlaced"; session_id: string; order: CopyTradeOrderSummary }
  | { kind: "OrderFilled"; session_id: string; order_id: string; fill_price: number; slippage_bps: number }
  | { kind: "OrderFailed"; session_id: string; order_id: string; error: string }
  | { kind: "SessionPaused"; session_id: string }
  | { kind: "SessionResumed"; session_id: string }
  | { kind: "SessionStopped"; session_id: string; reason: string | null }
  | { kind: "BalanceUpdate"; balance: string };

// Copy-Trade Dashboard (spec 16)

export interface SessionStats {
  total_orders: number;
  filled_orders: number;
  failed_orders: number;
  pending_orders: number;
  canceled_orders: number;
  total_invested: number;
  total_returned: number;
  realized_pnl: number;
  unrealized_pnl: number;
  total_pnl: number;
  return_pct: number;
  win_count: number;
  loss_count: number;
  win_rate: number;
  avg_slippage_bps: number;
  max_slippage_bps: number;
  capital_utilization: number;
  runtime_seconds: number;
}

export interface CopyTradePosition {
  asset_id: string;
  question: string;
  outcome: string;
  category: string;
  resolved: boolean;
  buy_shares: number;
  sell_shares: number;
  net_shares: number;
  avg_entry_price: number;
  current_price: number;
  last_fill_price: number;
  cost_basis: number;
  current_value: number;
  unrealized_pnl: number;
  realized_pnl: number;
  order_count: number;
  source_traders: string[];
  last_order_at: string;
}

export interface CopyTradeSummary {
  active_sessions: number;
  total_pnl: number;
  total_return_pct: number;
  total_orders: number;
}

// PayoutRedemption Insights (spec 19)

export interface TraderRedemption {
  condition_id: string;
  question: string;
  outcome: string;
  payout_usdc: number;
  tx_hash: string;
  block_number: number;
  redeemed_at: string;
}

export interface MarketRedemption {
  redeemer: string;
  payout_usdc: number;
  tx_hash: string;
  block_number: number;
  redeemed_at: string;
}

export interface RedemptionsResponse<T> {
  redemptions: T[];
  total: number;
}
