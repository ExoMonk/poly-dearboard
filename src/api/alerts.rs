use std::collections::{HashMap, HashSet};
use std::env;
use std::time::{Duration, Instant};

use axum::{
    extract::{
        ws::{Message, WebSocket},
        Query, State, WebSocketUpgrade,
    },
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use super::types::CopyTradeUpdate;
use super::{markets, server::AppState};

// ---------------------------------------------------------------------------
// Alert types
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind")]
pub enum Alert {
    WhaleTrade {
        timestamp: String,
        exchange: String,
        side: String,
        trader: String,
        asset_id: String,
        usdc_amount: String,
        token_amount: String,
        tx_hash: String,
        block_number: u64,
        question: Option<String>,
        outcome: Option<String>,
    },
    MarketResolution {
        timestamp: String,
        condition_id: String,
        oracle: String,
        question_id: String,
        payout_numerators: Vec<String>,
        tx_hash: String,
        block_number: u64,
        question: Option<String>,
        winning_outcome: Option<String>,
        outcomes: Vec<String>,
        token_id: Option<String>,
    },
    FailedSettlement {
        tx_hash: String,
        block_number: u64,
        timestamp: String,
        from_address: String,
        to_contract: String,
        function_name: String,
        gas_used: String,
    },
}

// ---------------------------------------------------------------------------
// Live trade (broadcast to /ws/trades subscribers)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
pub struct LiveTrade {
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
    pub block_number: u64,
    #[serde(skip)]
    pub cache_key: String,
}

// ---------------------------------------------------------------------------
// rindexer webhook payload
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub(crate) struct WebhookPayload {
    event_name: String,
    event_data: Vec<serde_json::Value>,
    #[allow(dead_code)]
    network: String,
}

#[derive(Deserialize)]
struct TxInfo {
    #[serde(default)]
    transaction_hash: String,
    #[serde(default)]
    block_number: u64,
    #[serde(default)]
    block_timestamp: String,
}

// ---------------------------------------------------------------------------
// POST /webhooks/rindexer
// ---------------------------------------------------------------------------

pub async fn webhook_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<WebhookPayload>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Validate shared secret
    let expected = env::var("RINDEXER_WEBHOOK_SECRET").unwrap_or_default();
    if !expected.is_empty() {
        let provided = headers
            .get("x-rindexer-shared-secret")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if provided != expected {
            return Err((StatusCode::UNAUTHORIZED, "Invalid shared secret".into()));
        }
    }

    let ws_active = state
        .ws_subscriber_active
        .load(std::sync::atomic::Ordering::SeqCst);

    for event in &payload.event_data {
        let is_live = is_event_live(event);

        let mut alert = {
            let cache = state.market_cache.read().await;

            // Broadcast trades + queue metadata persistence.
            // When ws_subscriber is active, it owns trade broadcasts — webhook only persists metadata.
            if payload.event_name == "OrderFilled" && is_live {
                if let Some(live_trade) = build_live_trade(event, &cache) {
                    // Always persist metadata (regardless of ws_active)
                    if let Some(info) = cache.get(&live_trade.cache_key) {
                        let _ = state
                            .metadata_tx
                            .try_send((live_trade.asset_id.clone(), info.clone()));
                    }
                    // Only broadcast if WS subscriber is not active (fallback mode)
                    if !ws_active {
                        let _ = state.trade_tx.send(live_trade);
                    }
                }
            }

            match payload.event_name.as_str() {
                // Only produce whale alerts from webhook when WS subscriber is down
                "OrderFilled" if !ws_active => parse_order_filled(event, &cache),
                "OrderFilled" => None, // WS subscriber handles whale alerts
                "ConditionResolution" => parse_condition_resolution(event, &cache),
                _ => None,
            }
        };

        // Enrich resolution alerts on cache miss — query Gamma API by condition_id.
        // Drop resolutions we can't identify (old V1 markets, unknown conditions).
        if let Some(Alert::MarketResolution {
            ref condition_id,
            ref mut question,
            ref mut outcomes,
            ref mut winning_outcome,
            ref mut token_id,
            ref payout_numerators,
            ..
        }) = alert
        {
            if question.is_some() {
                tracing::info!("ConditionResolution enriched from cache: condition_id={condition_id}");
            } else {
                tracing::warn!("ConditionResolution cache miss: condition_id={condition_id}, trying Gamma API");
                if let Some((q, outs, tid)) =
                    fetch_resolution_context(&state.http, condition_id).await
                {
                    tracing::info!("ConditionResolution enriched from Gamma: condition_id={condition_id}");
                    let winner = payout_numerators
                        .iter()
                        .enumerate()
                        .find(|(_, n)| n.parse::<u64>().unwrap_or(0) > 0)
                        .and_then(|(i, _)| outs.get(i).cloned());

                    *question = Some(q);
                    *outcomes = outs;
                    *winning_outcome = winner;
                    if !tid.is_empty() {
                        *token_id = Some(tid);
                    }
                } else {
                    tracing::warn!(
                        "ConditionResolution Gamma miss: condition_id={condition_id} (broadcasting with raw data)"
                    );
                }
            }
        }

        if let Some(alert) = alert {
            if is_live {
                let _ = state.alert_tx.send(alert);
            } else {
                tracing::debug!("Backfill guard: suppressed alert for stale event");
            }
        }
    }

    Ok(StatusCode::OK)
}

/// Common fields extracted from an OrderFilled event.
struct TradeData<'a> {
    tx_info: TxInfo,
    side: &'static str,
    asset_id: &'a str,
    usdc_raw: &'a str,
    token_raw: &'a str,
    trader: &'a str,
    exchange: &'static str,
    key: String,
    info: Option<&'a markets::MarketInfo>,
}

fn parse_trade_data<'a>(
    event: &'a serde_json::Value,
    cache: &'a std::collections::HashMap<String, markets::MarketInfo>,
) -> Option<TradeData<'a>> {
    let tx_info: TxInfo = serde_json::from_value(
        event.get("transaction_information")?.clone(),
    )
    .ok()?;

    let maker_asset_id = event.get("makerAssetId")?.as_str()?;
    let taker_asset_id = event.get("takerAssetId")?.as_str()?;
    let maker_amount = event.get("makerAmountFilled")?.as_str()?;
    let taker_amount = event.get("takerAmountFilled")?.as_str()?;
    let maker = event.get("maker")?.as_str()?;

    let (side, asset_id, usdc_raw, token_raw) = if maker_asset_id == "0" {
        ("buy", taker_asset_id, maker_amount, taker_amount)
    } else if taker_asset_id == "0" {
        ("sell", maker_asset_id, taker_amount, maker_amount)
    } else {
        return None; // MINT
    };

    let contract = event
        .get("contract_address")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let exchange = if contract.eq_ignore_ascii_case("0xC5d563A36AE78145C45a50134d48A1215220f80a") {
        "neg_risk"
    } else {
        "ctf"
    };

    let key = markets::cache_key(asset_id);
    let info = cache.get(&key);

    Some(TradeData { tx_info, side, asset_id, usdc_raw, token_raw, trader: maker, exchange, key, info })
}

fn parse_order_filled(
    event: &serde_json::Value,
    cache: &std::collections::HashMap<String, markets::MarketInfo>,
) -> Option<Alert> {
    let td = parse_trade_data(event, cache)?;

    // Whale threshold: $25k USDC = 25_000_000_000 raw (6 decimals)
    let usdc_raw_n: u128 = td.usdc_raw.parse().unwrap_or(0);
    if usdc_raw_n < 25_000_000_000 {
        return None;
    }

    Some(Alert::WhaleTrade {
        timestamp: td.tx_info.block_timestamp,
        exchange: td.exchange.into(),
        side: td.side.into(),
        trader: td.trader.into(),
        asset_id: td.asset_id.into(),
        usdc_amount: format_usdc(td.usdc_raw),
        token_amount: format_usdc(td.token_raw),
        tx_hash: td.tx_info.transaction_hash,
        block_number: td.tx_info.block_number,
        question: td.info.map(|i| i.question.clone()),
        outcome: td.info.map(|i| i.outcome.clone()),
    })
}

fn build_live_trade(
    event: &serde_json::Value,
    cache: &std::collections::HashMap<String, markets::MarketInfo>,
) -> Option<LiveTrade> {
    let td = parse_trade_data(event, cache)?;

    let usdc_n: f64 = td.usdc_raw.parse().unwrap_or(0.0);
    let token_n: f64 = td.token_raw.parse().unwrap_or(0.0);
    let price = if token_n > 0.0 { usdc_n / token_n } else { 0.0 };

    Some(LiveTrade {
        tx_hash: td.tx_info.transaction_hash,
        block_timestamp: td.tx_info.block_timestamp,
        trader: td.trader.into(),
        side: td.side.into(),
        asset_id: td.info
            .map(|i| i.gamma_token_id.clone())
            .unwrap_or_else(|| markets::to_integer_id(td.asset_id)),
        amount: format_usdc(td.token_raw),
        price: format!("{price:.6}"),
        usdc_amount: format_usdc(td.usdc_raw),
        question: td.info.map(|i| i.question.clone()).unwrap_or_default(),
        outcome: td.info.map(|i| i.outcome.clone()).unwrap_or_default(),
        category: td.info.map(|i| i.category.clone()).unwrap_or_default(),
        block_number: td.tx_info.block_number,
        cache_key: td.key,
    })
}

fn parse_condition_resolution(
    event: &serde_json::Value,
    cache: &std::collections::HashMap<String, markets::MarketInfo>,
) -> Option<Alert> {
    let tx_info: TxInfo = serde_json::from_value(
        event.get("transaction_information")?.clone(),
    )
    .ok()?;

    let condition_id = event.get("conditionId")?.as_str()?;
    let oracle = event.get("oracle")?.as_str().unwrap_or("");
    let question_id = event.get("questionId")?.as_str().unwrap_or("");
    let numerators: Vec<String> = event
        .get("payoutNumerators")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    // Collect all cache entries matching this condition_id, sorted by outcome_index.
    // Compare without 0x prefix since on-chain events omit it but Gamma includes it.
    let bare_cid = condition_id.strip_prefix("0x").unwrap_or(condition_id);
    let mut matched: Vec<&markets::MarketInfo> = cache
        .values()
        .filter(|info| {
            info.condition_id.as_deref().is_some_and(|cid| {
                cid.strip_prefix("0x").unwrap_or(cid) == bare_cid
            })
        })
        .collect();
    matched.sort_by_key(|info| info.outcome_index);

    let question = matched.first().map(|info| info.question.clone());
    let outcomes: Vec<String> = matched.iter().map(|info| info.outcome.clone()).collect();
    let token_id = matched.first().map(|info| info.gamma_token_id.clone());

    // Determine winning outcome: index where payout_numerator > 0
    let winning_outcome = numerators
        .iter()
        .enumerate()
        .find(|(_, n)| n.parse::<u64>().unwrap_or(0) > 0)
        .and_then(|(i, _)| outcomes.get(i).cloned());

    Some(Alert::MarketResolution {
        timestamp: tx_info.block_timestamp,
        condition_id: condition_id.into(),
        oracle: oracle.into(),
        question_id: question_id.into(),
        payout_numerators: numerators,
        tx_hash: tx_info.transaction_hash,
        block_number: tx_info.block_number,
        question,
        winning_outcome,
        outcomes,
        token_id,
    })
}

/// Fallback: query Gamma API by condition_id when market cache misses.
/// Returns (question, outcomes, first_token_id).
///
/// Note: Gamma API silently ignores unknown filter params and returns default
/// paginated results, so we MUST verify the returned conditionId matches.
async fn fetch_resolution_context(
    http: &reqwest::Client,
    condition_id: &str,
) -> Option<(String, Vec<String>, String)> {
    let cid_hex = if condition_id.starts_with("0x") {
        condition_id.to_string()
    } else {
        format!("0x{condition_id}")
    };
    let url = format!(
        "https://gamma-api.polymarket.com/markets?condition_ids={}",
        cid_hex
    );
    let resp = http
        .get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .ok()?;

    let body: Vec<serde_json::Value> = resp.json().await.ok()?;

    // Find the market whose conditionId actually matches — Gamma may return
    // unrelated results if the filter param is silently ignored.
    // Compare both with and without 0x prefix since formats vary.
    let bare_id = condition_id.strip_prefix("0x").unwrap_or(condition_id);
    let market = body.iter().find(|m| {
        m.get("conditionId")
            .and_then(|v| v.as_str())
            .is_some_and(|cid| {
                let bare_cid = cid.strip_prefix("0x").unwrap_or(cid);
                bare_cid == bare_id
            })
    })?;

    let question = market.get("question")?.as_str()?.to_string();

    // outcomes and clobTokenIds are JSON-encoded string arrays
    let outcomes: Vec<String> = market
        .get("outcomes")
        .and_then(|v| v.as_str())
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();

    let token_ids: Vec<String> = market
        .get("clobTokenIds")
        .and_then(|v| v.as_str())
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();

    let token_id = token_ids.into_iter().next().unwrap_or_default();

    Some((question, outcomes, token_id))
}

fn format_usdc(raw: &str) -> String {
    let n: u128 = raw.parse().unwrap_or(0);
    let whole = n / 1_000_000;
    let frac = n % 1_000_000;
    format!("{whole}.{frac:06}")
}

/// Returns true if the event's block_timestamp is within 5 minutes of now.
/// During backfill, events are historical and should not trigger live broadcasts.
///
/// block_timestamp arrives from rindexer as a decimal string of unix epoch
/// seconds (e.g. "1705312496"), serialized from alloy U256.
fn is_event_live(event: &serde_json::Value) -> bool {
    let Some(tx_info) = event.get("transaction_information") else {
        tracing::warn!("is_event_live: no transaction_information field");
        return true; // fail-open: broadcast if we can't determine staleness
    };
    let ts_val = tx_info.get("block_timestamp");
    // block_timestamp is Option<U256> in rindexer — could be null or missing
    let ts_str = ts_val.and_then(|v| v.as_str());
    if ts_str.is_none() {
        tracing::debug!(
            "is_event_live: block_timestamp missing or not a string (value: {:?})",
            ts_val
        );
        return true; // fail-open: broadcast if timestamp unavailable
    }
    let ts_str = ts_str.unwrap();
    let Ok(ts) = (if let Some(hex) = ts_str.strip_prefix("0x") {
        i64::from_str_radix(hex, 16)
    } else {
        ts_str.parse::<i64>()
    }) else {
        tracing::warn!("is_event_live: failed to parse block_timestamp: {ts_str}");
        return true; // fail-open
    };
    let now = chrono::Utc::now().timestamp();
    let delta = (now - ts).abs();
    if delta >= 300 {
        tracing::debug!("is_event_live: stale event (delta={delta}s, ts={ts})");
    }
    delta < 300
}

// ---------------------------------------------------------------------------
// GET /ws/alerts — WebSocket upgrade
// ---------------------------------------------------------------------------

pub async fn ws_handler(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, state.alert_tx.subscribe()))
}

async fn handle_ws(mut socket: WebSocket, mut rx: broadcast::Receiver<Alert>) {
    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Ok(alert) => {
                        let json = match serde_json::to_string(&alert) {
                            Ok(j) => j,
                            Err(_) => continue,
                        };
                        if socket.send(Message::Text(json.into())).await.is_err() {
                            break; // Client disconnected
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("WebSocket client lagged, skipped {n} alerts");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            // Handle incoming messages (ping/pong/close)
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {} // Ignore text/binary from client
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// GET /ws/trades — WebSocket upgrade (market-filtered trade stream)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct TradesWsParams {
    token_ids: String,
    /// Optional comma-separated trader addresses for server-side filtering.
    /// When set, only trades from these addresses are forwarded.
    traders: Option<String>,
}

pub async fn trades_ws_handler(
    State(state): State<AppState>,
    Query(params): Query<TradesWsParams>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let prefixes: HashSet<String> = params
        .token_ids
        .split(',')
        .map(|s| markets::cache_key(s.trim()))
        .collect();
    let trader_filter: HashSet<String> = params
        .traders
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    ws.on_upgrade(move |socket| {
        handle_trades_ws(socket, state.trade_tx.subscribe(), prefixes, trader_filter)
    })
}

async fn handle_trades_ws(
    mut socket: WebSocket,
    mut rx: broadcast::Receiver<LiveTrade>,
    prefixes: HashSet<String>,
    trader_filter: HashSet<String>,
) {
    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Ok(trade) => {
                        if !prefixes.contains(&trade.cache_key) {
                            continue;
                        }
                        if !trader_filter.is_empty()
                            && !trader_filter.contains(&trade.trader.to_lowercase())
                        {
                            continue;
                        }
                        let json = match serde_json::to_string(&trade) {
                            Ok(j) => j,
                            Err(_) => continue,
                        };
                        if socket.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::debug!("Trades WS client lagged, skipped {n} trades");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// GET /ws/signals — Trader-filtered signal feed with convergence detection
// ---------------------------------------------------------------------------
// JWT is passed via query param since WebSocket upgrade can't send headers.
// Accepted tradeoff: token appears in logs. Data sensitivity is low (public
// trader addresses). See spec 09-polylab-evolution.md for design rationale.

#[derive(Clone, Serialize)]
#[serde(tag = "kind")]
pub enum SignalMessage {
    Trade(LiveTrade),
    Convergence(ConvergenceAlert),
    Lag { dropped: u64 },
}

#[derive(Clone, Serialize)]
pub struct ConvergenceAlert {
    pub question: String,
    pub asset_id: String,
    pub outcome: String,
    pub traders: Vec<String>,
    pub trader_count: u32,
    pub window_seconds: u64,
    pub side: String,
    pub total_usdc: f64,
}

#[derive(Deserialize)]
pub struct SignalWsParams {
    list_id: Option<String>,
    top_n: Option<u32>,
    token: String,
}

pub async fn signals_ws_handler(
    State(state): State<AppState>,
    Query(params): Query<SignalWsParams>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, (axum::http::StatusCode, String)> {
    // Validate JWT from query param before upgrading
    let owner = super::auth::validate_jwt(&params.token, &state.jwt_secret)
        .map_err(|_| (axum::http::StatusCode::UNAUTHORIZED, "Invalid token".into()))?;

    // Mutual exclusion: exactly one of list_id or top_n
    if params.list_id.is_some() && params.top_n.is_some() {
        return Err((axum::http::StatusCode::BAD_REQUEST, "Specify list_id or top_n, not both".into()));
    }

    let trader_set: HashSet<String> = if let Some(ref list_id) = params.list_id {
        // Load from SQLite list
        let conn = state.user_db.lock().unwrap_or_else(|p| p.into_inner());
        let addrs = super::db::get_list_member_addresses(&conn, list_id, &owner)
            .map_err(|_| (axum::http::StatusCode::NOT_FOUND, "List not found".into()))?;
        addrs.into_iter().collect()
    } else {
        // Top N from ClickHouse leaderboard (default 20)
        let top_n = params.top_n.unwrap_or(20).clamp(1, 50);
        let exclude = super::routes::exclude_clause();
        let query = format!(
            "WITH resolved AS (
                SELECT asset_id, toNullable(toFloat64(resolved_price)) AS resolved_price
                FROM poly_dearboard.resolved_prices FINAL
            )
            SELECT toString(p.trader) AS address
            FROM poly_dearboard.trader_positions p
            LEFT JOIN (SELECT asset_id, latest_price FROM poly_dearboard.asset_latest_price FINAL) AS lp ON p.asset_id = lp.asset_id
            LEFT JOIN resolved rp ON p.asset_id = rp.asset_id
            WHERE p.trader NOT IN ({exclude})
            GROUP BY p.trader
            ORDER BY sum((p.sell_usdc - p.buy_usdc) + (p.buy_amount - p.sell_amount) * coalesce(rp.resolved_price, toFloat64(lp.latest_price))) DESC
            LIMIT {top_n}"
        );

        #[derive(clickhouse::Row, serde::Deserialize)]
        struct Addr {
            address: String,
        }

        let rows: Vec<Addr> = state.db.query(&query)
            .fetch_all::<Addr>()
            .await
            .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        rows.into_iter().map(|r| r.address).collect()
    };

    if trader_set.is_empty() {
        return Err((axum::http::StatusCode::BAD_REQUEST, "No traders found".into()));
    }

    Ok(ws.on_upgrade(move |socket| {
        handle_signal_ws(socket, state.trade_tx.subscribe(), trader_set)
    }))
}

struct ConvergenceDetector {
    // asset_id → [(trader, timestamp, side, usdc_amount)]
    recent_trades: HashMap<String, Vec<(String, Instant, String, f64)>>,
    window: Duration,
    threshold: usize,
    last_alert: HashMap<String, Instant>,
    max_assets: usize,
}

impl ConvergenceDetector {
    fn new() -> Self {
        Self {
            recent_trades: HashMap::new(),
            window: Duration::from_secs(300), // 5 minutes
            threshold: 2,
            last_alert: HashMap::new(),
            max_assets: 500,
        }
    }

    fn record_trade(&mut self, trade: &LiveTrade) -> Option<ConvergenceAlert> {
        let now = Instant::now();
        let asset_id = &trade.asset_id;
        let usdc: f64 = trade.usdc_amount.parse().unwrap_or(0.0);

        // Insert into recent_trades
        let entries = self.recent_trades.entry(asset_id.clone()).or_default();
        entries.push((trade.trader.clone(), now, trade.side.clone(), usdc));

        // Evict old entries for this asset
        entries.retain(|(_, ts, _, _)| now.duration_since(*ts) < self.window);

        // Count distinct traders
        let distinct_traders: HashSet<&str> = entries.iter().map(|(t, _, _, _)| t.as_str()).collect();
        let count = distinct_traders.len();

        if count >= self.threshold {
            // Dedup: don't re-fire for same asset within 60s
            if let Some(last) = self.last_alert.get(asset_id) {
                if now.duration_since(*last) < Duration::from_secs(60) {
                    return None;
                }
            }
            self.last_alert.insert(asset_id.clone(), now);

            // Build alert
            let traders: Vec<String> = distinct_traders.into_iter().map(String::from).collect();
            let total_usdc: f64 = entries.iter().map(|(_, _, _, u)| u).sum();

            // Dominant side
            let buy_count = entries.iter().filter(|(_, _, s, _)| s == "buy").count();
            let sell_count = entries.len() - buy_count;
            let side = if buy_count >= sell_count { "BUY" } else { "SELL" };

            return Some(ConvergenceAlert {
                question: trade.question.clone(),
                asset_id: asset_id.clone(),
                outcome: trade.outcome.clone(),
                traders,
                trader_count: count as u32,
                window_seconds: 300,
                side: side.into(),
                total_usdc,
            });
        }

        None
    }

    /// Periodic cleanup: remove entries older than window across all assets.
    fn sweep(&mut self) {
        let now = Instant::now();
        self.recent_trades.retain(|_, entries| {
            entries.retain(|(_, ts, _, _)| now.duration_since(*ts) < self.window);
            !entries.is_empty()
        });
        // Also clean stale alert dedup entries
        self.last_alert.retain(|_, ts| now.duration_since(*ts) < Duration::from_secs(120));

        // Hard cap on tracked assets — drop oldest if exceeded
        while self.recent_trades.len() > self.max_assets {
            // Find the oldest entry across all assets and remove that asset
            if let Some(oldest_key) = self
                .recent_trades
                .iter()
                .min_by_key(|(_, entries)| {
                    entries.iter().map(|(_, ts, _, _)| *ts).min().unwrap_or(now)
                })
                .map(|(k, _)| k.clone())
            {
                self.recent_trades.remove(&oldest_key);
            } else {
                break;
            }
        }
    }
}

async fn handle_signal_ws(
    mut socket: WebSocket,
    mut rx: broadcast::Receiver<LiveTrade>,
    trader_set: HashSet<String>,
) {
    let mut detector = ConvergenceDetector::new();
    let mut sweep_interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
    sweep_interval.tick().await; // skip immediate tick

    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Ok(trade) => {
                        if !trader_set.contains(&trade.trader.to_lowercase()) {
                            continue;
                        }

                        // Send trade signal
                        let msg = SignalMessage::Trade(trade.clone());
                        let json = match serde_json::to_string(&msg) {
                            Ok(j) => j,
                            Err(_) => continue,
                        };
                        if socket.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }

                        // Check convergence
                        if let Some(alert) = detector.record_trade(&trade) {
                            let alert_msg = SignalMessage::Convergence(alert);
                            if let Ok(json) = serde_json::to_string(&alert_msg) {
                                if socket.send(Message::Text(json.into())).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Signal WS client lagged, skipped {n} trades");
                        let lag_msg = SignalMessage::Lag { dropped: n };
                        if let Ok(json) = serde_json::to_string(&lag_msg) {
                            let _ = socket.send(Message::Text(json.into())).await;
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            _ = sweep_interval.tick() => {
                detector.sweep();
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Copy-trade updates WebSocket (/ws/copytrade?token=JWT)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CopyTradeWsParams {
    token: String,
}

pub async fn copytrade_ws_handler(
    State(state): State<AppState>,
    Query(params): Query<CopyTradeWsParams>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let owner = super::auth::validate_jwt(&params.token, &state.jwt_secret)
        .map_err(|_| (StatusCode::UNAUTHORIZED, "Invalid token".into()))?;

    let rx = state.copytrade_update_tx.subscribe();
    Ok(ws.on_upgrade(move |socket| handle_copytrade_ws(socket, rx, owner)))
}

async fn handle_copytrade_ws(
    mut socket: WebSocket,
    mut rx: broadcast::Receiver<CopyTradeUpdate>,
    owner: String,
) {
    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Ok(update) => {
                        // Filter by owner
                        if update.owner() != owner {
                            continue;
                        }
                        if let Ok(json) = serde_json::to_string(&update) {
                            if socket.send(Message::Text(json.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Copytrade WS lagged, dropped {n} updates");
                    }
                    Err(_) => break,
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }
}
