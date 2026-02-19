use std::collections::HashSet;
use std::env;

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

    for event in &payload.event_data {
        let mut alert = {
            let cache = state.market_cache.read().await;

            // Broadcast ALL trades for /ws/trades subscribers (before whale filter)
            if payload.event_name == "OrderFilled" {
                if let Some(live_trade) = build_live_trade(event, &cache) {
                    let _ = state.trade_tx.send(live_trade);
                }
            }

            match payload.event_name.as_str() {
                "OrderFilled" => parse_order_filled(event, &cache),
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
            if question.is_none() {
                if let Some((q, outs, tid)) =
                    fetch_resolution_context(&state.http, condition_id).await
                {
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
                    tracing::debug!(
                        "Dropping unresolvable ConditionResolution: condition_id={condition_id}"
                    );
                    alert = None;
                }
            }
        }

        if let Some(alert) = alert {
            // Ignore send errors — just means no WebSocket subscribers
            let _ = state.alert_tx.send(alert);
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

    // Collect all cache entries matching this condition_id, sorted by outcome_index
    let mut matched: Vec<&markets::MarketInfo> = cache
        .values()
        .filter(|info| info.condition_id.as_deref() == Some(condition_id))
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
    let url = format!(
        "https://gamma-api.polymarket.com/markets?condition_id={}",
        condition_id
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
    let market = body.iter().find(|m| {
        m.get("conditionId")
            .and_then(|v| v.as_str())
            .is_some_and(|cid| cid == condition_id)
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
    ws.on_upgrade(move |socket| {
        handle_trades_ws(socket, state.trade_tx.subscribe(), prefixes)
    })
}

async fn handle_trades_ws(
    mut socket: WebSocket,
    mut rx: broadcast::Receiver<LiveTrade>,
    prefixes: HashSet<String>,
) {
    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Ok(trade) => {
                        if !prefixes.contains(&trade.cache_key) {
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
                        tracing::warn!("Trades WS client lagged, skipped {n} trades");
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
