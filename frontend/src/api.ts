import type {
  LeaderboardResponse,
  TraderSummary,
  TradesResponse,
  HealthResponse,
  HotMarketsResponse,
  LiveFeedResponse,
  PositionsResponse,
  PnlChartResponse,
  ResolvedMarket,
  SortColumn,
  SortOrder,
  Timeframe,
} from "./types";

const BASE = "/api";

export async function fetchLeaderboard(params: {
  sort?: SortColumn;
  order?: SortOrder;
  limit?: number;
  offset?: number;
  timeframe?: Timeframe;
}): Promise<LeaderboardResponse> {
  const sp = new URLSearchParams();
  if (params.sort) sp.set("sort", params.sort);
  if (params.order) sp.set("order", params.order);
  if (params.limit) sp.set("limit", String(params.limit));
  if (params.offset !== undefined) sp.set("offset", String(params.offset));
  if (params.timeframe && params.timeframe !== "all") sp.set("timeframe", params.timeframe);
  const res = await fetch(`${BASE}/leaderboard?${sp}`);
  if (!res.ok) throw new Error(`Leaderboard fetch failed: ${res.status}`);
  return res.json();
}

export async function fetchTrader(address: string): Promise<TraderSummary> {
  const res = await fetch(`${BASE}/trader/${address}`);
  if (!res.ok) throw new Error(`Trader fetch failed: ${res.status}`);
  return res.json();
}

export async function fetchTraderTrades(
  address: string,
  params: { limit?: number; offset?: number; side?: string },
): Promise<TradesResponse> {
  const sp = new URLSearchParams();
  if (params.limit) sp.set("limit", String(params.limit));
  if (params.offset !== undefined) sp.set("offset", String(params.offset));
  if (params.side) sp.set("side", params.side);
  const res = await fetch(`${BASE}/trader/${address}/trades?${sp}`);
  if (!res.ok) throw new Error(`Trades fetch failed: ${res.status}`);
  return res.json();
}

export async function fetchHealth(): Promise<HealthResponse> {
  const res = await fetch(`${BASE}/health`);
  if (!res.ok) throw new Error(`Health fetch failed: ${res.status}`);
  return res.json();
}

export async function fetchHotMarkets(params?: {
  period?: string;
  limit?: number;
}): Promise<HotMarketsResponse> {
  const sp = new URLSearchParams();
  if (params?.period) sp.set("period", params.period);
  if (params?.limit) sp.set("limit", String(params.limit));
  const res = await fetch(`${BASE}/markets/hot?${sp}`);
  if (!res.ok) throw new Error(`Hot markets fetch failed: ${res.status}`);
  return res.json();
}

export async function fetchRecentTrades(params?: {
  limit?: number;
  token_id?: string;
}): Promise<LiveFeedResponse> {
  const sp = new URLSearchParams();
  if (params?.limit) sp.set("limit", String(params.limit));
  if (params?.token_id) sp.set("token_id", params.token_id);
  const res = await fetch(`${BASE}/trades/recent?${sp}`);
  if (!res.ok) throw new Error(`Recent trades fetch failed: ${res.status}`);
  return res.json();
}

export async function fetchTraderPositions(address: string): Promise<PositionsResponse> {
  const res = await fetch(`${BASE}/trader/${address}/positions`);
  if (!res.ok) throw new Error(`Positions fetch failed: ${res.status}`);
  return res.json();
}

export async function fetchPnlChart(address: string): Promise<PnlChartResponse> {
  const res = await fetch(`${BASE}/trader/${address}/pnl-chart`);
  if (!res.ok) throw new Error(`PnL chart fetch failed: ${res.status}`);
  return res.json();
}

export async function fetchMarketResolve(
  tokenIds: string,
): Promise<Record<string, ResolvedMarket>> {
  const sp = new URLSearchParams({ token_ids: tokenIds });
  const res = await fetch(`${BASE}/market/resolve?${sp}`);
  if (!res.ok) throw new Error(`Market resolve failed: ${res.status}`);
  return res.json();
}
