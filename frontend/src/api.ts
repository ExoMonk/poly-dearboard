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
  SmartMoneyResponse,
  TraderProfile,
  BacktestResponse,
  BacktestTimeframe,
  CopyPortfolioResponse,
  TraderList,
  TraderListDetail,
  TradingWalletInfo,
  WalletGenerateResponse,
  ImportWalletResponse,
  DeriveCredentialsResponse,
  WalletBalance,
  ApprovalResult,
  DepositAddresses,
  DepositStatus,
  SortColumn,
  SortOrder,
  Timeframe,
  PnlTimeframe,
} from "./types";

const BASE = import.meta.env.VITE_API_URL || "/api";
const JWT_KEY = "pd_jwt";

// -- Auth helpers --

function authHeaders(): HeadersInit {
  const token = localStorage.getItem(JWT_KEY);
  return token ? { Authorization: `Bearer ${token}` } : {};
}

async function authFetch(url: string, init?: RequestInit): Promise<Response> {
  const headers = { ...authHeaders(), ...init?.headers };
  return fetch(url, { ...init, headers });
}

export async function fetchNonce(
  address: string,
): Promise<{ nonce: string; issuedAt: string }> {
  const res = await fetch(
    `${BASE}/auth/nonce?address=${encodeURIComponent(address)}`,
  );
  if (!res.ok) throw new Error(`Nonce fetch failed: ${res.status}`);
  return res.json();
}

export async function verifySignature(body: {
  address: string;
  signature: string;
  nonce: string;
  issued_at: string;
}): Promise<{ token: string; address: string }> {
  const res = await fetch(`${BASE}/auth/verify`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`Signature verification failed: ${res.status}`);
  return res.json();
}

// -- Protected endpoints --

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
  const res = await authFetch(`${BASE}/leaderboard?${sp}`);
  if (!res.ok) throw new Error(`Leaderboard fetch failed: ${res.status}`);
  return res.json();
}

export async function fetchTrader(address: string): Promise<TraderSummary> {
  const res = await authFetch(`${BASE}/trader/${address}`);
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
  const res = await authFetch(`${BASE}/trader/${address}/trades?${sp}`);
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
  const res = await authFetch(`${BASE}/markets/hot?${sp}`);
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
  const res = await authFetch(`${BASE}/trades/recent?${sp}`);
  if (!res.ok) throw new Error(`Recent trades fetch failed: ${res.status}`);
  return res.json();
}

export async function fetchTraderPositions(address: string): Promise<PositionsResponse> {
  const res = await authFetch(`${BASE}/trader/${address}/positions`);
  if (!res.ok) throw new Error(`Positions fetch failed: ${res.status}`);
  return res.json();
}

export async function fetchPnlChart(address: string, timeframe?: PnlTimeframe): Promise<PnlChartResponse> {
  const sp = new URLSearchParams();
  if (timeframe && timeframe !== "all") sp.set("timeframe", timeframe);
  const res = await authFetch(`${BASE}/trader/${address}/pnl-chart?${sp}`);
  if (!res.ok) throw new Error(`PnL chart fetch failed: ${res.status}`);
  return res.json();
}

export async function fetchMarketResolve(
  tokenIds: string,
): Promise<Record<string, ResolvedMarket>> {
  const sp = new URLSearchParams({ token_ids: tokenIds });
  const res = await authFetch(`${BASE}/market/resolve?${sp}`);
  if (!res.ok) throw new Error(`Market resolve failed: ${res.status}`);
  return res.json();
}

export async function fetchTraderProfile(address: string): Promise<TraderProfile> {
  const res = await authFetch(`${BASE}/trader/${address}/profile`);
  if (!res.ok) throw new Error(`Profile fetch failed: ${res.status}`);
  return res.json();
}

export async function fetchBacktest(params: {
  topN?: number;
  listId?: string;
  timeframe: BacktestTimeframe;
  initialCapital?: number;
  copyPct?: number;
}): Promise<BacktestResponse> {
  const body: Record<string, unknown> = {
    timeframe: params.timeframe,
    initial_capital: params.initialCapital,
    copy_pct: params.copyPct,
  };
  if (params.listId) {
    body.list_id = params.listId;
  } else {
    body.top_n = params.topN;
  }
  const res = await authFetch(`${BASE}/lab/backtest`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`Backtest fetch failed: ${res.status}`);
  return res.json();
}

export async function fetchSmartMoney(params?: {
  timeframe?: Timeframe;
  top?: number;
}): Promise<SmartMoneyResponse> {
  const sp = new URLSearchParams();
  if (params?.timeframe && params.timeframe !== "all")
    sp.set("timeframe", params.timeframe);
  if (params?.top) sp.set("top", String(params.top));
  const qs = sp.toString();
  const res = await authFetch(`${BASE}/smart-money${qs ? `?${qs}` : ""}`);
  if (!res.ok) throw new Error(`Smart money fetch failed: ${res.status}`);
  return res.json();
}

export async function fetchCopyPortfolio(params?: {
  top?: number;
  listId?: string;
}): Promise<CopyPortfolioResponse> {
  const sp = new URLSearchParams();
  if (params?.listId) sp.set("list_id", params.listId);
  else if (params?.top) sp.set("top", String(params.top));
  const qs = sp.toString();
  const res = await authFetch(`${BASE}/lab/copy-portfolio${qs ? `?${qs}` : ""}`);
  if (!res.ok) throw new Error(`Copy portfolio fetch failed: ${res.status}`);
  return res.json();
}

// -- Trader Lists --

export async function fetchTraderLists(): Promise<TraderList[]> {
  const res = await authFetch(`${BASE}/lists`);
  if (!res.ok) throw new Error(`Lists fetch failed: ${res.status}`);
  return res.json();
}

export async function fetchTraderListDetail(id: string): Promise<TraderListDetail> {
  const res = await authFetch(`${BASE}/lists/${id}`);
  if (!res.ok) throw new Error(`List detail fetch failed: ${res.status}`);
  return res.json();
}

export async function createTraderList(name: string): Promise<TraderList> {
  const res = await authFetch(`${BASE}/lists`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ name }),
  });
  if (!res.ok) throw new Error(`Create list failed: ${res.status}`);
  return res.json();
}

export async function renameTraderList(id: string, name: string): Promise<void> {
  const res = await authFetch(`${BASE}/lists/${id}`, {
    method: "PATCH",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ name }),
  });
  if (!res.ok) throw new Error(`Rename list failed: ${res.status}`);
}

export async function deleteTraderList(id: string): Promise<void> {
  const res = await authFetch(`${BASE}/lists/${id}`, { method: "DELETE" });
  if (!res.ok) throw new Error(`Delete list failed: ${res.status}`);
}

export async function addListMembers(id: string, addresses: string[]): Promise<void> {
  const res = await authFetch(`${BASE}/lists/${id}/members`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ addresses }),
  });
  if (!res.ok) throw new Error(`Add members failed: ${res.status}`);
}

export async function removeListMembers(id: string, addresses: string[]): Promise<void> {
  const res = await authFetch(`${BASE}/lists/${id}/members`, {
    method: "DELETE",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ addresses }),
  });
  if (!res.ok) throw new Error(`Remove members failed: ${res.status}`);
}

// -- Trading Wallets (multi-wallet, up to 3 per user) --

export async function fetchWallets(): Promise<TradingWalletInfo[]> {
  const res = await authFetch(`${BASE}/wallets`);
  if (!res.ok) throw new Error(`Wallets fetch failed: ${res.status}`);
  return res.json();
}

export async function generateWallet(): Promise<WalletGenerateResponse> {
  const res = await authFetch(`${BASE}/wallets/generate`, { method: "POST" });
  if (!res.ok) throw new Error(`Wallet generate failed: ${res.status}`);
  return res.json();
}

export async function importWallet(privateKey: string): Promise<ImportWalletResponse> {
  const res = await authFetch(`${BASE}/wallets/import`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ private_key: privateKey }),
  });
  if (!res.ok) throw new Error(`Wallet import failed: ${res.status}`);
  return res.json();
}

export async function deriveCredentials(walletId: string): Promise<DeriveCredentialsResponse> {
  const res = await authFetch(`${BASE}/wallets/${walletId}/derive-credentials`, { method: "POST" });
  if (!res.ok) throw new Error(`Derive credentials failed: ${res.status}`);
  return res.json();
}

export async function deleteWallet(walletId: string): Promise<void> {
  const res = await authFetch(`${BASE}/wallets/${walletId}`, { method: "DELETE" });
  if (!res.ok) throw new Error(`Delete wallet failed: ${res.status}`);
}

// -- Wallet Funding (spec 14) --

export async function fetchWalletBalance(walletId: string): Promise<WalletBalance> {
  const res = await authFetch(`${BASE}/wallets/${walletId}/balance`);
  if (!res.ok) throw new Error(`Balance fetch failed: ${res.status}`);
  return res.json();
}

export async function approveExchanges(walletId: string): Promise<ApprovalResult> {
  const res = await authFetch(`${BASE}/wallets/${walletId}/approve`, { method: "POST" });
  if (!res.ok) {
    const body = await res.text();
    throw new Error(body || `Approve failed: ${res.status}`);
  }
  return res.json();
}

export async function fetchDepositAddress(walletId: string): Promise<DepositAddresses> {
  const res = await authFetch(`${BASE}/wallets/${walletId}/deposit-address`);
  if (!res.ok) throw new Error(`Deposit address fetch failed: ${res.status}`);
  return res.json();
}

export async function fetchDepositStatus(walletId: string): Promise<DepositStatus> {
  const res = await authFetch(`${BASE}/wallets/${walletId}/deposit-status`);
  if (!res.ok) throw new Error(`Deposit status fetch failed: ${res.status}`);
  return res.json();
}

export class NotFoundError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "NotFoundError";
  }
}

// -- Copy-Trade Engine --

export async function createSession(body: import("./types").CreateSessionRequest): Promise<import("./types").CopyTradeSession> {
  const res = await authFetch(`${BASE}/copytrade/sessions`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) {
    const text = await res.text();
    throw new Error(text || `Create session failed: ${res.status}`);
  }
  return res.json();
}

export async function listSessions(): Promise<import("./types").CopyTradeSession[]> {
  const res = await authFetch(`${BASE}/copytrade/sessions`);
  if (!res.ok) throw new Error(`List sessions failed: ${res.status}`);
  return res.json();
}

export async function getSession(id: string): Promise<import("./types").CopyTradeSession> {
  const res = await authFetch(`${BASE}/copytrade/sessions/${id}`);
  if (!res.ok) throw new Error(`Get session failed: ${res.status}`);
  return res.json();
}

export async function updateSession(id: string, action: "pause" | "resume" | "stop"): Promise<import("./types").CopyTradeSession> {
  const res = await authFetch(`${BASE}/copytrade/sessions/${id}`, {
    method: "PATCH",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ action }),
  });
  if (!res.ok) {
    const text = await res.text();
    throw new Error(text || `Update session failed: ${res.status}`);
  }
  return res.json();
}

export async function deleteSession(id: string): Promise<void> {
  const res = await authFetch(`${BASE}/copytrade/sessions/${id}`, { method: "DELETE" });
  if (!res.ok) throw new Error(`Delete session failed: ${res.status}`);
}

export async function listSessionOrders(sessionId: string, limit = 50, offset = 0): Promise<import("./types").CopyTradeOrder[]> {
  const res = await authFetch(`${BASE}/copytrade/sessions/${sessionId}/orders?limit=${limit}&offset=${offset}`);
  if (!res.ok) throw new Error(`List orders failed: ${res.status}`);
  return res.json();
}

export async function closePosition(sessionId: string, assetId: string): Promise<{ order_id: string; status: string }> {
  const res = await authFetch(`${BASE}/copytrade/close-position`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ session_id: sessionId, asset_id: assetId }),
  });
  if (!res.ok) {
    const text = await res.text();
    throw new Error(text || `Close position failed: ${res.status}`);
  }
  return res.json();
}

export async function getSessionStats(sessionId: string): Promise<import("./types").SessionStats> {
  const res = await authFetch(`${BASE}/copytrade/sessions/${sessionId}/stats`);
  if (!res.ok) throw new Error(`Get session stats failed: ${res.status}`);
  return res.json();
}

export async function getSessionPositions(sessionId: string): Promise<import("./types").CopyTradePosition[]> {
  const res = await authFetch(`${BASE}/copytrade/sessions/${sessionId}/positions`);
  if (!res.ok) throw new Error(`Get session positions failed: ${res.status}`);
  return res.json();
}

export async function getCopyTradeSummary(): Promise<import("./types").CopyTradeSummary> {
  const res = await authFetch(`${BASE}/copytrade/summary`);
  if (!res.ok) throw new Error(`Get copytrade summary failed: ${res.status}`);
  return res.json();
}

export async function getActiveTraders(): Promise<string[]> {
  const res = await authFetch(`${BASE}/copytrade/active-traders`);
  if (!res.ok) throw new Error(`Get active traders failed: ${res.status}`);
  return res.json();
}
