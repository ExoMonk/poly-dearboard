use std::collections::HashSet;
use std::time::{Duration, Instant};

use alloy_primitives::B256;
use alloy_sol_types::{SolEvent, sol};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::{broadcast, watch};
use tokio_tungstenite::tungstenite::Message;

use super::alerts::LiveTrade;
use super::markets;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const CTF_EXCHANGE: &str = "0x4bFb41d5B3570DeFd03C39a9A4D8dE6Bd8B8982E";
const NEGRISK_EXCHANGE: &str = "0xC5d563A36AE78145C45a50134d48A1215220f80a";
const RECONNECT_BASE_DELAY: Duration = Duration::from_secs(2);
const RECONNECT_MAX_DELAY: Duration = Duration::from_secs(60);
const HEALTH_LOG_INTERVAL: Duration = Duration::from_secs(60);
const MAX_TRACKED_ADDRESSES_WARN: usize = 200;

// ---------------------------------------------------------------------------
// ABI
// ---------------------------------------------------------------------------

sol! {
    event OrderFilled(
        bytes32 indexed orderHash,
        address indexed maker,
        address indexed taker,
        uint256 makerAssetId,
        uint256 takerAssetId,
        uint256 makerAmountFilled,
        uint256 takerAmountFilled,
        uint256 fee
    );
}

// ---------------------------------------------------------------------------
// JSON-RPC types for eth_subscribe
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct SubscriptionResponse {
    result: Option<String>,
    error: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct SubscriptionNotification {
    params: Option<SubscriptionParams>,
}

#[derive(Deserialize)]
struct SubscriptionParams {
    result: LogEntry,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LogEntry {
    #[allow(dead_code)]
    address: String,
    topics: Vec<String>,
    data: String,
    transaction_hash: String,
    block_number: String,
    #[serde(default)]
    removed: bool,
}

// ---------------------------------------------------------------------------
// RPC helper for eth_getBlockByNumber (block timestamp resolution)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct RpcResponse<T> {
    result: Option<T>,
}

#[derive(Deserialize)]
struct BlockResult {
    timestamp: String,
}

async fn get_block_timestamp(
    http: &reqwest::Client,
    rpc_url: &str,
    block_hex: &str,
) -> Option<u64> {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "eth_getBlockByNumber",
        "params": [block_hex, false],
        "id": 1
    });
    let resp = http
        .post(rpc_url)
        .json(&body)
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .ok()?;
    let rpc: RpcResponse<BlockResult> = resp.json().await.ok()?;
    let ts_hex = rpc.result?.timestamp;
    u64::from_str_radix(ts_hex.trim_start_matches("0x"), 16).ok()
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub async fn run(
    copytrade_tx: broadcast::Sender<LiveTrade>,
    mut trader_watch_rx: watch::Receiver<HashSet<String>>,
    market_cache: markets::MarketCache,
    http: reqwest::Client,
    rpc_url: String,
) {
    let ws_url = std::env::var("POLYGON_WS_URL").unwrap_or_else(|_| {
        "".into()
    });

    // Wait for market cache to warm before subscribing
    tokio::time::sleep(Duration::from_secs(10)).await;

    loop {
        // Wait for non-empty address set
        let addrs = trader_watch_rx.borrow_and_update().clone();
        if addrs.is_empty() {
            tracing::info!("WS subscriber: no tracked addresses, waiting for sessions...");
            if trader_watch_rx.changed().await.is_err() {
                tracing::info!("WS subscriber: watch channel closed, shutting down");
                break;
            }
            continue;
        }

        if addrs.len() > MAX_TRACKED_ADDRESSES_WARN {
            tracing::warn!(
                "WS subscriber: {} tracked addresses exceeds recommended max {}",
                addrs.len(),
                MAX_TRACKED_ADDRESSES_WARN
            );
        }

        tracing::info!(
            "WS subscriber: subscribing for {} tracked address(es)",
            addrs.len()
        );

        subscribe_and_process(
            &addrs,
            &copytrade_tx,
            &mut trader_watch_rx,
            &market_cache,
            &http,
            &rpc_url,
            &ws_url,
        )
        .await;
    }
}

// ---------------------------------------------------------------------------
// Subscribe and process loop
// ---------------------------------------------------------------------------

async fn subscribe_and_process(
    addrs: &HashSet<String>,
    copytrade_tx: &broadcast::Sender<LiveTrade>,
    trader_watch_rx: &mut watch::Receiver<HashSet<String>>,
    market_cache: &markets::MarketCache,
    http: &reqwest::Client,
    rpc_url: &str,
    ws_url: &str,
) {
    let mut backoff = RECONNECT_BASE_DELAY;

    loop {
        // Check if address set changed while reconnecting
        if trader_watch_rx.has_changed().unwrap_or(false) {
            let new_addrs = trader_watch_rx.borrow_and_update().clone();
            if new_addrs.is_empty() || new_addrs != *addrs {
                tracing::info!(
                    "WS subscriber: addresses changed during reconnect, returning to resubscribe"
                );
                return;
            }
        }

        tracing::info!(
            "WS subscriber: connecting to {}",
            &ws_url[..ws_url.len().min(60)]
        );

        match tokio_tungstenite::connect_async(ws_url).await {
            Ok((ws_stream, _)) => {
                backoff = RECONNECT_BASE_DELAY;
                let (mut write, mut read) = ws_stream.split();

                // Build topic filter with maker addresses (topic[2])
                let topic0 = format!("0x{}", hex::encode(OrderFilled::SIGNATURE_HASH));
                let maker_topics = build_maker_topic_filter(addrs);

                let subscribe_msg = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "eth_subscribe",
                    "params": ["logs", {
                        "address": [CTF_EXCHANGE, NEGRISK_EXCHANGE],
                        "topics": [topic0, serde_json::Value::Null, maker_topics]
                    }]
                });

                tracing::debug!(
                    "WS subscriber: sending eth_subscribe with {} maker filter(s)",
                    addrs.len()
                );

                if let Err(e) = write.send(Message::Text(subscribe_msg.to_string())).await {
                    tracing::warn!("WS subscriber: failed to send subscribe: {e}");
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(RECONNECT_MAX_DELAY);
                    continue;
                }

                // Wait for subscription confirmation
                let sub_id = match read.next().await {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<SubscriptionResponse>(&text) {
                            Ok(resp) if resp.result.is_some() => {
                                let id = resp.result.unwrap();
                                tracing::info!(
                                    "WS subscriber: active (sub_id={id}, tracking {} address(es))",
                                    addrs.len()
                                );
                                id
                            }
                            Ok(resp) => {
                                tracing::warn!(
                                    "WS subscriber: subscription rejected: {:?}",
                                    resp.error
                                );
                                tokio::time::sleep(backoff).await;
                                backoff = (backoff * 2).min(RECONNECT_MAX_DELAY);
                                continue;
                            }
                            Err(e) => {
                                tracing::warn!("WS subscriber: unexpected response: {e} — {text}");
                                tokio::time::sleep(backoff).await;
                                backoff = (backoff * 2).min(RECONNECT_MAX_DELAY);
                                continue;
                            }
                        }
                    }
                    other => {
                        tracing::warn!("WS subscriber: no subscription response: {other:?}");
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(RECONNECT_MAX_DELAY);
                        continue;
                    }
                };

                // Inner message loop
                let connected_at = Instant::now();
                let mut event_count: u64 = 0;
                let mut last_health_log = Instant::now();
                let mut cached_block: Option<(u64, u64)> = None;

                loop {
                    tokio::select! {
                        msg = read.next() => {
                            match msg {
                                Some(Ok(Message::Text(text))) => {
                                    // Health log
                                    if last_health_log.elapsed() >= HEALTH_LOG_INTERVAL {
                                        let receivers = copytrade_tx.receiver_count();
                                        tracing::info!(
                                            "WS subscriber health: {event_count} events, uptime={}s, sub={sub_id}, addrs={}, receivers={receivers}",
                                            connected_at.elapsed().as_secs(),
                                            addrs.len(),
                                        );
                                        if receivers == 0 {
                                            tracing::warn!("WS subscriber: copytrade_tx has zero receivers while addresses are tracked");
                                        }
                                        last_health_log = Instant::now();
                                    }

                                    let notification: SubscriptionNotification =
                                        match serde_json::from_str(&text) {
                                            Ok(n) => n,
                                            Err(_) => continue,
                                        };

                                    let Some(params) = notification.params else {
                                        continue;
                                    };
                                    let log_entry = params.result;

                                    if log_entry.removed {
                                        tracing::debug!("WS subscriber: skipping removed log");
                                        continue;
                                    }

                                    event_count += 1;

                                    if let Some((trade, _usdc_raw)) = decode_order_filled(
                                        &log_entry,
                                        market_cache,
                                        http,
                                        rpc_url,
                                        &mut cached_block,
                                    ).await {
                                        let _ = copytrade_tx.send(trade);
                                    }
                                }
                                Some(Ok(Message::Ping(data))) => {
                                    let _ = write.send(Message::Pong(data)).await;
                                }
                                Some(Ok(Message::Close(_))) | None => {
                                    tracing::warn!(
                                        "WS subscriber: disconnected (uptime={}s, events={event_count})",
                                        connected_at.elapsed().as_secs()
                                    );
                                    break;
                                }
                                Some(Err(e)) => {
                                    tracing::warn!(
                                        "WS subscriber: error: {e} (uptime={}s, events={event_count})",
                                        connected_at.elapsed().as_secs()
                                    );
                                    break;
                                }
                                _ => {}
                            }
                        }
                        result = trader_watch_rx.changed() => {
                            if result.is_err() {
                                tracing::info!("WS subscriber: watch channel closed");
                                return;
                            }
                            // Address set changed — unsubscribe and return to outer loop
                            let new_addrs = trader_watch_rx.borrow_and_update().clone();
                            tracing::info!(
                                "WS subscriber: address set changed ({} → {} addrs), resubscribing",
                                addrs.len(),
                                new_addrs.len()
                            );
                            // Send eth_unsubscribe (best-effort)
                            let unsub_msg = serde_json::json!({
                                "jsonrpc": "2.0",
                                "id": 2,
                                "method": "eth_unsubscribe",
                                "params": [sub_id]
                            });
                            let _ = write.send(Message::Text(unsub_msg.to_string())).await;
                            return;
                        }
                    }
                }

                // WS disconnected — outer loop will reconnect
            }
            Err(e) => {
                tracing::warn!("WS subscriber: connection failed: {e}");
            }
        }

        tracing::info!("WS subscriber: reconnecting in {}s", backoff.as_secs());
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(RECONNECT_MAX_DELAY);
    }
}

// ---------------------------------------------------------------------------
// Build topic filter for maker addresses (topic[2])
// ---------------------------------------------------------------------------

fn build_maker_topic_filter(addrs: &HashSet<String>) -> serde_json::Value {
    let padded: Vec<serde_json::Value> = addrs
        .iter()
        .map(|addr| {
            let bare = addr.trim_start_matches("0x");
            serde_json::Value::String(format!("0x{bare:0>64}"))
        })
        .collect();
    serde_json::Value::Array(padded)
}

// ---------------------------------------------------------------------------
// Decode a raw log entry into a LiveTrade
// ---------------------------------------------------------------------------

async fn decode_order_filled(
    log_entry: &LogEntry,
    market_cache: &markets::MarketCache,
    http: &reqwest::Client,
    rpc_url: &str,
    cached_block: &mut Option<(u64, u64)>,
) -> Option<(LiveTrade, u128)> {
    let topics: Vec<B256> = log_entry
        .topics
        .iter()
        .filter_map(|t| t.parse::<B256>().ok())
        .collect();

    if topics.len() < 4 {
        tracing::debug!("WS subscriber: log has {} topics, expected 4", topics.len());
        return None;
    }

    let data_bytes = hex::decode(log_entry.data.trim_start_matches("0x")).ok()?;
    let decoded = OrderFilled::decode_raw_log(topics.iter().copied(), &data_bytes).ok()?;

    let maker_asset_id = decoded.makerAssetId;
    let taker_asset_id = decoded.takerAssetId;
    let maker_amount = decoded.makerAmountFilled;
    let taker_amount = decoded.takerAmountFilled;
    let maker = decoded.maker;

    let (side, asset_id, usdc_raw, token_raw) = if maker_asset_id.is_zero() {
        ("buy", taker_asset_id, maker_amount, taker_amount)
    } else if taker_asset_id.is_zero() {
        ("sell", maker_asset_id, taker_amount, maker_amount)
    } else {
        tracing::debug!("WS subscriber: both asset IDs non-zero, skipping");
        return None;
    };

    let usdc_raw_u128: u128 = usdc_raw.try_into().ok()?;
    let token_raw_u128: u128 = token_raw.try_into().ok()?;

    let block_number =
        u64::from_str_radix(log_entry.block_number.trim_start_matches("0x"), 16).unwrap_or(0);

    let block_timestamp = match cached_block {
        Some((cached_num, cached_ts)) if *cached_num == block_number => *cached_ts,
        _ => {
            let ts = get_block_timestamp(http, rpc_url, &log_entry.block_number)
                .await
                .unwrap_or_else(|| chrono::Utc::now().timestamp() as u64);
            *cached_block = Some((block_number, ts));
            ts
        }
    };

    let usdc_whole = usdc_raw_u128 / 1_000_000;
    let usdc_frac = usdc_raw_u128 % 1_000_000;
    let usdc_str = format!("{usdc_whole}.{usdc_frac:06}");

    let token_whole = token_raw_u128 / 1_000_000;
    let token_frac = token_raw_u128 % 1_000_000;
    let token_str = format!("{token_whole}.{token_frac:06}");

    let price = if token_raw_u128 > 0 {
        usdc_raw_u128 as f64 / token_raw_u128 as f64
    } else {
        0.0
    };

    let asset_id_str = asset_id.to_string();
    let cache_key = markets::cache_key(&asset_id_str);
    let cache = market_cache.read().await;
    let info = cache.get(&cache_key);

    let trade = LiveTrade {
        tx_hash: log_entry.transaction_hash.clone(),
        block_timestamp: block_timestamp.to_string(),
        trader: format!("{:?}", maker),
        side: side.into(),
        asset_id: info
            .map(|i| i.gamma_token_id.clone())
            .unwrap_or_else(|| markets::to_integer_id(&asset_id_str)),
        amount: token_str,
        price: format!("{price:.6}"),
        usdc_amount: usdc_str,
        question: info.map(|i| i.question.clone()).unwrap_or_default(),
        outcome: info.map(|i| i.outcome.clone()).unwrap_or_default(),
        category: info.map(|i| i.category.clone()).unwrap_or_default(),
        block_number,
        cache_key,
    };

    Some((trade, usdc_raw_u128))
}
