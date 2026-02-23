use std::collections::{HashMap, HashSet, VecDeque};
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rust_decimal::Decimal;
use std::sync::Mutex;
use tokio::sync::{RwLock, broadcast, mpsc};

use alloy::signers::Signer as _;
use polymarket_client_sdk::auth::state::Authenticated;
use polymarket_client_sdk::auth::{Credentials, Normal};
use polymarket_client_sdk::clob::types::request::PriceRequest;
use polymarket_client_sdk::clob::types::{Amount, OrderStatusType, OrderType, Side, SignatureType};
use polymarket_client_sdk::clob::{Client, Config};
use polymarket_client_sdk::types::U256;

use super::alerts::LiveTrade;
use super::db::{self, CopyTradeOrderRow, CopyTradeSessionRow};
use super::types::{
    CopyOrderType, CopyTradeOrderSummary, CopyTradeUpdate, OrderStatus, SessionStatus,
};

// ---------------------------------------------------------------------------
// Public types shared with server.rs / copytrade.rs
// ---------------------------------------------------------------------------

pub enum CopyTradeCommand {
    Start { session_id: String, owner: String },
    Pause { session_id: String },
    Resume { session_id: String },
    Stop { session_id: String },
}

pub struct ClobClientState {
    pub client: Client<Authenticated<Normal>>,
    pub signer: alloy::signers::local::LocalSigner<k256::ecdsa::SigningKey>,
}

// ---------------------------------------------------------------------------
// Internal types
// ---------------------------------------------------------------------------

struct ActiveSession {
    config: CopyTradeSessionRow,
    traders: HashSet<String>,
    trader_count: usize,
    recent_orders: HashMap<String, Instant>, // "asset_id:side" → last order time (dedup)
    consecutive_failures: u32,
    cooldown_until: Option<Instant>,
    remaining_capital: f64,
    // Position tracking: asset_id → (net_shares, last_fill_price)
    positions: HashMap<String, (f64, f64)>,
    open_gtc_orders: HashMap<String, (String, Instant, f64)>, // clob_order_id → (our_id, placed_at, usdc)
}

// Rate limit: global sliding window across all sessions (shared CLOB account)
const MAX_ORDERS_PER_MINUTE: usize = 10;
const DEDUP_WINDOW: Duration = Duration::from_secs(30);
const COOLDOWN_DURATION: Duration = Duration::from_secs(60);
const MAX_CONSECUTIVE_FAILURES: u32 = 3;
const MIN_ORDER_USDC: f64 = 1.0;
const GTC_TIMEOUT: Duration = Duration::from_secs(3600);
const HEALTH_INTERVAL: Duration = Duration::from_secs(60);

// ---------------------------------------------------------------------------
// CLOB client initialization
// ---------------------------------------------------------------------------

pub async fn init_clob_client(
    user_db: &Arc<Mutex<rusqlite::Connection>>,
    encryption_key: &[u8; 32],
    owner: &str,
) -> Result<ClobClientState, String> {
    // Load the first credentialed wallet for this owner
    let row = {
        let conn = user_db.lock().unwrap_or_else(|p| p.into_inner());
        let wallets = db::get_trading_wallets(&conn, owner)
            .map_err(|e| format!("DB error loading wallets: {e}"))?;
        wallets
            .into_iter()
            .find(|w| w.clob_api_key.is_some())
            .ok_or_else(|| "No credentialed wallet found".to_string())?
    };

    // Decrypt private key
    let user_key = super::crypto::derive_user_key(encryption_key, owner);
    let pk_bytes = super::crypto::decrypt_secret(
        &user_key,
        &row.encrypted_key,
        &row.key_nonce,
        owner.as_bytes(),
    )?;
    let pk_hex = format!("0x{}", hex::encode(&pk_bytes));

    // Decrypt CLOB credentials
    let cred_blob = row.clob_credentials.ok_or("Missing CLOB credentials")?;
    let cred_nonce = row.clob_nonce.ok_or("Missing CLOB nonce")?;
    let cred_json_bytes =
        super::crypto::decrypt_secret(&user_key, &cred_blob, &cred_nonce, owner.as_bytes())?;
    let cred_json: serde_json::Value =
        serde_json::from_slice(&cred_json_bytes).map_err(|e| format!("Invalid cred JSON: {e}"))?;

    let api_key_str = row.clob_api_key.ok_or("Missing CLOB API key")?;
    let api_key_uuid =
        uuid::Uuid::parse_str(&api_key_str).map_err(|e| format!("Invalid API key UUID: {e}"))?;
    let secret = cred_json["secret"]
        .as_str()
        .ok_or("Missing secret in credentials")?
        .to_string();
    let passphrase = cred_json["passphrase"]
        .as_str()
        .ok_or("Missing passphrase in credentials")?
        .to_string();

    let credentials = Credentials::new(api_key_uuid, secret, passphrase);

    // Create signer
    let signer = alloy::signers::local::LocalSigner::from_str(&pk_hex)
        .map_err(|e| format!("Signer creation failed: {e}"))?
        .with_chain_id(Some(polymarket_client_sdk::POLYGON));

    // Build authenticated client
    let config = Config::builder().use_server_time(true).build();
    let client = Client::new("https://clob.polymarket.com", config)
        .map_err(|e| format!("CLOB client error: {e}"))?
        .authentication_builder(&signer)
        .credentials(credentials)
        .signature_type(SignatureType::Proxy)
        .authenticate()
        .await
        .map_err(|e| format!("CLOB auth error: {e}"))?;

    Ok(ClobClientState { client, signer })
}

// ---------------------------------------------------------------------------
// Trader resolution
// ---------------------------------------------------------------------------

pub async fn resolve_session_traders(
    user_db: &Arc<Mutex<rusqlite::Connection>>,
    ch_db: &clickhouse::Client,
    session: &CopyTradeSessionRow,
) -> Result<HashSet<String>, String> {
    if let Some(ref list_id) = session.list_id {
        let conn = user_db.lock().unwrap_or_else(|p| p.into_inner());
        let addrs = db::get_list_member_addresses(&conn, list_id, &session.owner)
            .map_err(|_| "List not found".to_string())?;
        Ok(addrs.into_iter().map(|a| a.to_lowercase()).collect())
    } else if let Some(top_n) = session.top_n {
        let top_n = top_n.clamp(1, 50);
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

        let rows: Vec<Addr> = ch_db
            .query(&query)
            .fetch_all::<Addr>()
            .await
            .map_err(|e| format!("ClickHouse error: {e}"))?;
        Ok(rows.into_iter().map(|r| r.address).collect())
    } else {
        Err("Session has neither list_id nor top_n".into())
    }
}

// ---------------------------------------------------------------------------
// Main engine loop
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub async fn copytrade_engine_loop(
    mut trade_rx: broadcast::Receiver<LiveTrade>,
    mut cmd_rx: mpsc::Receiver<CopyTradeCommand>,
    update_tx: broadcast::Sender<CopyTradeUpdate>,
    clob_client: Arc<RwLock<Option<ClobClientState>>>,
    user_db: Arc<Mutex<rusqlite::Connection>>,
    encryption_key: Arc<[u8; 32]>,
    ch_db: clickhouse::Client,
    trader_watch_tx: tokio::sync::watch::Sender<std::collections::HashSet<String>>,
) {
    let mut sessions: HashMap<String, ActiveSession> = HashMap::new();
    let mut health_interval = tokio::time::interval(HEALTH_INTERVAL);
    health_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut order_timestamps: VecDeque<Instant> = VecDeque::new();

    // On startup: reload running sessions
    {
        let running = {
            let conn = user_db.lock().unwrap_or_else(|p| p.into_inner());
            db::get_running_sessions(&conn).unwrap_or_default()
        };
        for session_row in running {
            tracing::info!("Reloading running session {}", session_row.id);
            match resolve_session_traders(&user_db, &ch_db, &session_row).await {
                Ok(traders) => {
                    let trader_count = traders.len();
                    // Restore positions from DB so sells and circuit breaker work after restart
                    let positions = {
                        let conn = user_db.lock().unwrap_or_else(|p| p.into_inner());
                        db::get_session_positions(&conn, &session_row.id).unwrap_or_default()
                    };
                    if !positions.is_empty() {
                        tracing::info!(
                            "Restored {} positions for session {}",
                            positions.len(),
                            session_row.id
                        );
                    }
                    sessions.insert(
                        session_row.id.clone(),
                        ActiveSession {
                            remaining_capital: session_row.remaining_capital,
                            config: session_row,
                            traders,
                            trader_count,
                            recent_orders: HashMap::new(),
                            consecutive_failures: 0,
                            cooldown_until: None,
                            positions,
                            open_gtc_orders: HashMap::new(),
                        },
                    );
                }
                Err(e) => {
                    tracing::error!("Failed to reload session traders: {e}");
                }
            }
        }
        if !sessions.is_empty() {
            tracing::info!("Reloaded {} running session(s)", sessions.len());
            publish_tracked_addresses(&sessions, &trader_watch_tx);
        }
    }

    loop {
        tokio::select! {
            result = trade_rx.recv() => {
                match result {
                    Ok(trade) => {
                        for session in sessions.values_mut().filter(|s| {
                            SessionStatus::from_str(&s.config.status) == Some(SessionStatus::Running)
                        }) {
                            process_trade(
                                &trade,
                                session,
                                &clob_client,
                                &user_db,
                                &update_tx,
                                &mut order_timestamps,
                            )
                            .await;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Copytrade engine lagged, dropped {n} trades");
                    }
                    Err(_) => {
                        tracing::error!("copytrade_live_tx channel closed, engine shutting down");
                        break;
                    }
                }
            }

            Some(cmd) = cmd_rx.recv() => {
                match cmd {
                    CopyTradeCommand::Start { session_id, owner } => {
                        handle_start(
                            &session_id, &owner, &mut sessions, &clob_client,
                            &user_db, &encryption_key, &ch_db, &update_tx,
                        ).await;
                        publish_tracked_addresses(&sessions, &trader_watch_tx);
                    }
                    CopyTradeCommand::Pause { session_id } => {
                        if let Some(session) = sessions.get_mut(&session_id) {
                            session.config.status = "paused".to_string();
                            let _ = update_tx.send(CopyTradeUpdate::SessionPaused {
                                session_id,
                                owner: session.config.owner.clone(),
                            });
                            publish_tracked_addresses(&sessions, &trader_watch_tx);
                        }
                    }
                    CopyTradeCommand::Resume { session_id } => {
                        if let Some(session) = sessions.get_mut(&session_id) {
                            // Refresh trader set on resume
                            if let Ok(traders) = resolve_session_traders(&user_db, &ch_db, &session.config).await {
                                session.trader_count = traders.len();
                                session.traders = traders;
                            }
                            session.config.status = "running".to_string();
                            session.consecutive_failures = 0;
                            session.cooldown_until = None;
                            let _ = update_tx.send(CopyTradeUpdate::SessionResumed {
                                session_id,
                                owner: session.config.owner.clone(),
                            });
                            publish_tracked_addresses(&sessions, &trader_watch_tx);
                        }
                    }
                    CopyTradeCommand::Stop { session_id } => {
                        if let Some(session) = sessions.remove(&session_id) {
                            // Cancel open GTC orders
                            if !session.open_gtc_orders.is_empty() {
                                let clob = clob_client.read().await;
                                if let Some(ref cs) = *clob {
                                    let ids: Vec<&str> = session.open_gtc_orders.keys().map(|s| s.as_str()).collect();
                                    match cs.client.cancel_orders(&ids).await {
                                        Ok(resp) => tracing::info!("Canceled {} GTC orders on stop", resp.canceled.len()),
                                        Err(e) => tracing::warn!("Failed to cancel GTC orders: {e}"),
                                    }
                                }
                            }
                            let _ = update_tx.send(CopyTradeUpdate::SessionStopped {
                                session_id,
                                reason: Some("user".to_string()),
                                owner: session.config.owner.clone(),
                            });
                            publish_tracked_addresses(&sessions, &trader_watch_tx);
                        }
                    }
                }
            }

            _ = health_interval.tick() => {
                health_check(&mut sessions, &clob_client, &user_db, &update_tx, &trader_watch_tx).await;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Command: Start
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn handle_start(
    session_id: &str,
    owner: &str,
    sessions: &mut HashMap<String, ActiveSession>,
    clob_client: &Arc<RwLock<Option<ClobClientState>>>,
    user_db: &Arc<Mutex<rusqlite::Connection>>,
    encryption_key: &[u8; 32],
    ch_db: &clickhouse::Client,
    update_tx: &broadcast::Sender<CopyTradeUpdate>,
) {
    // Load session from DB
    let session_row = {
        let conn = user_db.lock().unwrap_or_else(|p| p.into_inner());
        match db::get_copytrade_session(&conn, session_id, owner) {
            Ok(Some(row)) => row,
            Ok(None) => {
                tracing::error!("Session {session_id} not found in DB");
                return;
            }
            Err(e) => {
                tracing::error!("DB error loading session {session_id}: {e}");
                return;
            }
        }
    };

    // Initialize CLOB client if not yet done (skip for simulation-only)
    if !session_row.simulate {
        let needs_init = clob_client.read().await.is_none();
        if needs_init {
            match init_clob_client(user_db, encryption_key, owner).await {
                Ok(cs) => {
                    *clob_client.write().await = Some(cs);
                    tracing::info!("CLOB client initialized for owner {owner}");
                }
                Err(e) => {
                    tracing::error!("Failed to init CLOB client: {e}");
                    // Mark session as stopped
                    let conn = user_db.lock().unwrap_or_else(|p| p.into_inner());
                    let _ = db::update_session_status(&conn, session_id, "stopped");
                    let _ = update_tx.send(CopyTradeUpdate::SessionStopped {
                        session_id: session_id.to_string(),
                        reason: Some(format!("CLOB init failed: {e}")),
                        owner: owner.to_string(),
                    });
                    return;
                }
            }
        }
    }

    // Resolve traders
    match resolve_session_traders(user_db, ch_db, &session_row).await {
        Ok(traders) => {
            let trader_count = traders.len();
            tracing::info!(
                "Session {session_id} started: {} traders, simulate={}",
                trader_count,
                session_row.simulate
            );
            sessions.insert(
                session_id.to_string(),
                ActiveSession {
                    remaining_capital: session_row.remaining_capital,
                    config: session_row,
                    traders,
                    trader_count,
                    recent_orders: HashMap::new(),
                    consecutive_failures: 0,
                    cooldown_until: None,
                    positions: HashMap::new(),
                    open_gtc_orders: HashMap::new(),
                },
            );
        }
        Err(e) => {
            tracing::error!("Failed to resolve traders for session {session_id}: {e}");
            let conn = user_db.lock().unwrap_or_else(|p| p.into_inner());
            let _ = db::update_session_status(&conn, session_id, "stopped");
            let _ = update_tx.send(CopyTradeUpdate::SessionStopped {
                session_id: session_id.to_string(),
                reason: Some(format!("Trader resolution failed: {e}")),
                owner: owner.to_string(),
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Trade processing (the 11-step pipeline)
// ---------------------------------------------------------------------------

async fn process_trade(
    trade: &LiveTrade,
    session: &mut ActiveSession,
    clob_client: &Arc<RwLock<Option<ClobClientState>>>,
    user_db: &Arc<Mutex<rusqlite::Connection>>,
    update_tx: &broadcast::Sender<CopyTradeUpdate>,
    order_timestamps: &mut VecDeque<Instant>,
) {
    let sid = &session.config.id;

    // 1. FILTER — is trader in watched set?
    if !session.traders.contains(&trade.trader.to_lowercase()) {
        return;
    }

    // 2. COOLDOWN
    if let Some(until) = session.cooldown_until {
        if Instant::now() < until {
            tracing::debug!("Session {sid} in cooldown, skipping trade");
            return;
        }
        session.cooldown_until = None;
        session.consecutive_failures = 0;
    }

    // 3. DEDUP — same asset_id + side within 30s?
    let dedup_key = format!("{}:{}", trade.asset_id, trade.side);
    if let Some(last) = session.recent_orders.get(&dedup_key) {
        if last.elapsed() < DEDUP_WINDOW {
            tracing::debug!("Dedup: already ordered {dedup_key} within 30s");
            return;
        }
    }

    // Parse amounts
    let source_price = match trade.price.parse::<f64>() {
        Ok(p) if p > 0.0 => p,
        _ => return,
    };
    let trade_usdc = match trade.usdc_amount.parse::<f64>() {
        Ok(u) if u > 0.0 => u,
        _ => return,
    };

    // Parse side early — needed for sizing logic
    let side = match trade.side.to_lowercase().as_str() {
        "buy" => Side::Buy,
        "sell" => Side::Sell,
        _ => return,
    };

    // 4. SIZING (direction-aware)
    let copy_pct = session.config.copy_pct;
    let order_usdc = match side {
        Side::Buy => {
            let per_trader_budget = if session.trader_count > 0 {
                session.remaining_capital * copy_pct / session.trader_count as f64
            } else {
                0.0
            };
            (trade_usdc * copy_pct)
                .min(per_trader_budget)
                .min(session.config.max_position_usdc)
        }
        Side::Sell => {
            // For sells, size based on our position, not capital
            let (cur_shares, _) = session
                .positions
                .get(&trade.asset_id)
                .copied()
                .unwrap_or((0.0, 0.0));
            if cur_shares <= 0.0 {
                return; // No position to sell
            }
            // Mirror the source trader's sell proportion, capped by our holdings
            let source_shares = trade_usdc / source_price;
            let our_sell_shares = (source_shares * copy_pct).min(cur_shares);
            our_sell_shares * source_price // Convert to USDC equivalent for the order
        }
        _ => return,
    };

    if order_usdc < MIN_ORDER_USDC {
        return;
    }

    // 5. BALANCE (only check for buys — sells add capital)
    if matches!(side, Side::Buy) && session.remaining_capital < order_usdc {
        tracing::warn!(
            "Session {sid}: insufficient capital ({:.2} < {:.2})",
            session.remaining_capital,
            order_usdc
        );
        if session.remaining_capital < MIN_ORDER_USDC {
            // Auto-pause on empty balance
            session.config.status = "paused".to_string();
            let conn = user_db.lock().unwrap_or_else(|p| p.into_inner());
            let _ = db::update_session_status(&conn, &session.config.id, "paused");
            let _ = update_tx.send(CopyTradeUpdate::SessionPaused {
                session_id: sid.clone(),
                owner: session.config.owner.clone(),
            });
        }
        return;
    }

    // 6. RATE LIMIT (global)
    let now = Instant::now();
    order_timestamps.retain(|t| now.duration_since(*t) < Duration::from_secs(60));
    if order_timestamps.len() >= MAX_ORDERS_PER_MINUTE {
        tracing::warn!("Rate limit: {MAX_ORDERS_PER_MINUTE} orders/min exceeded");
        return;
    }

    let order_type =
        CopyOrderType::from_str(&session.config.order_type).unwrap_or(CopyOrderType::FOK);

    // 7. SLIPPAGE CHECK + 8. EXECUTE
    let order_id = uuid::Uuid::new_v4().to_string();
    let created_at = chrono::Utc::now().to_rfc3339();

    let submitted = if session.config.simulate {
        execute_simulated(
            trade,
            session,
            order_usdc,
            source_price,
            side,
            &order_id,
            &created_at,
            clob_client,
            user_db,
            update_tx,
        )
        .await
    } else {
        execute_live(
            trade,
            session,
            order_usdc,
            source_price,
            side,
            order_type,
            &order_id,
            &created_at,
            clob_client,
            user_db,
            update_tx,
        )
        .await
    };

    // Only record dedup + rate limit on actual submission
    if submitted {
        session.recent_orders.insert(dedup_key, now);
        order_timestamps.push_back(now);
    }
}

// ---------------------------------------------------------------------------
// Simulation execution (paper trading with real prices)
// ---------------------------------------------------------------------------

async fn execute_simulated(
    trade: &LiveTrade,
    session: &mut ActiveSession,
    order_usdc: f64,
    source_price: f64,
    side: Side,
    order_id: &str,
    created_at: &str,
    clob_client: &Arc<RwLock<Option<ClobClientState>>>,
    user_db: &Arc<Mutex<rusqlite::Connection>>,
    update_tx: &broadcast::Sender<CopyTradeUpdate>,
) -> bool {
    let sid = &session.config.id;

    // Try to fetch real CLOB price for realistic simulation
    let current_price = fetch_clob_price(clob_client, &trade.asset_id, side).await;

    // Simulate fill: use real price if available, otherwise source price + random slippage
    let fill_price = if let Some(cp) = current_price {
        cp
    } else {
        // Small random slippage ±0-50bps
        let slippage_factor = 1.0 + (rand::random::<f64>() - 0.5) * 0.01; // ±0.5%
        source_price * slippage_factor
    };

    // Check slippage
    let slippage_bps = match side {
        Side::Buy => (fill_price - source_price) / source_price * 10000.0,
        Side::Sell => (source_price - fill_price) / source_price * 10000.0,
        _ => return false,
    };

    if slippage_bps > session.config.max_slippage_bps as f64 {
        tracing::info!(
            "Session {sid}: slippage {slippage_bps:.0}bps exceeds max {}bps (simulated)",
            session.config.max_slippage_bps
        );
        return false;
    }

    let size_shares = order_usdc / fill_price;

    // Position-aware capital tracking
    let actual_usdc;
    let actual_shares;
    match side {
        Side::Buy => {
            // Buy: spend USDC, receive shares
            actual_usdc = order_usdc;
            actual_shares = size_shares;
            session.remaining_capital -= actual_usdc;
            let (cur_shares, _) = session
                .positions
                .get(&trade.asset_id)
                .copied()
                .unwrap_or((0.0, 0.0));
            let new_shares = cur_shares + actual_shares;
            session
                .positions
                .insert(trade.asset_id.clone(), (new_shares, fill_price));
        }
        Side::Sell => {
            // Sell: only if we hold shares in this asset
            let (cur_shares, _) = session
                .positions
                .get(&trade.asset_id)
                .copied()
                .unwrap_or((0.0, 0.0));
            if cur_shares <= 0.0 {
                tracing::debug!("SIM {sid}: no position to sell for {}", trade.asset_id);
                return false;
            }
            // Sell up to what we hold
            actual_shares = size_shares.min(cur_shares);
            actual_usdc = actual_shares * fill_price;
            session.remaining_capital += actual_usdc; // Receive USDC from sale
            let new_shares = cur_shares - actual_shares;
            if new_shares < 0.001 {
                session.positions.remove(&trade.asset_id);
            } else {
                session
                    .positions
                    .insert(trade.asset_id.clone(), (new_shares, fill_price));
            }
        }
        _ => return false,
    }

    // Record order
    let order_row = CopyTradeOrderRow {
        id: order_id.to_string(),
        session_id: sid.clone(),
        source_tx_hash: trade.tx_hash.clone(),
        source_trader: trade.trader.clone(),
        clob_order_id: None,
        asset_id: trade.asset_id.clone(),
        side: trade.side.clone(),
        price: fill_price,
        source_price,
        size_usdc: actual_usdc,
        size_shares: Some(actual_shares),
        status: OrderStatus::Simulated.as_str().to_string(),
        error_message: None,
        fill_price: Some(fill_price),
        slippage_bps: Some(slippage_bps),
        tx_hash: None,
        created_at: created_at.to_string(),
        updated_at: created_at.to_string(),
    };

    {
        let conn = user_db.lock().unwrap_or_else(|p| p.into_inner());
        if let Err(e) = db::insert_copytrade_order(&conn, &order_row) {
            tracing::error!("Failed to insert simulated order: {e}");
            return false;
        }
    }

    tracing::info!(
        "SIM {sid}: {} {:.2} USDC ({:.4} shares) on {} @ {:.4} (source {:.4}, slippage {:.0}bps)",
        trade.side,
        actual_usdc,
        actual_shares,
        trade.asset_id,
        fill_price,
        source_price,
        slippage_bps
    );

    // Broadcast updates
    let _ = update_tx.send(CopyTradeUpdate::OrderPlaced {
        session_id: sid.clone(),
        order: CopyTradeOrderSummary {
            id: order_id.to_string(),
            asset_id: trade.asset_id.clone(),
            side: trade.side.clone(),
            size_usdc: order_usdc,
            price: fill_price,
            source_trader: trade.trader.clone(),
            simulate: true,
        },
        owner: session.config.owner.clone(),
    });
    let _ = update_tx.send(CopyTradeUpdate::OrderFilled {
        session_id: sid.clone(),
        order_id: order_id.to_string(),
        fill_price,
        slippage_bps,
        owner: session.config.owner.clone(),
    });

    session.consecutive_failures = 0;
    true
}

// ---------------------------------------------------------------------------
// Live execution (real CLOB orders)
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn execute_live(
    trade: &LiveTrade,
    session: &mut ActiveSession,
    order_usdc: f64,
    source_price: f64,
    side: Side,
    order_type: CopyOrderType,
    order_id: &str,
    created_at: &str,
    clob_client: &Arc<RwLock<Option<ClobClientState>>>,
    user_db: &Arc<Mutex<rusqlite::Connection>>,
    update_tx: &broadcast::Sender<CopyTradeUpdate>,
) -> bool {
    let sid = session.config.id.clone();

    // 7. SLIPPAGE CHECK — fetch current CLOB price
    let current_price = match fetch_clob_price(clob_client, &trade.asset_id, side).await {
        Some(p) => p,
        None => {
            tracing::warn!(
                "Session {sid}: couldn't fetch CLOB price for {}, skipping",
                trade.asset_id
            );
            return false;
        }
    };

    let slippage_bps = match side {
        Side::Buy => (current_price - source_price) / source_price * 10000.0,
        Side::Sell => (source_price - current_price) / source_price * 10000.0,
        _ => return false,
    };

    if slippage_bps > session.config.max_slippage_bps as f64 {
        tracing::info!(
            "Session {sid}: slippage {slippage_bps:.0}bps exceeds max {}bps",
            session.config.max_slippage_bps
        );
        return false;
    }

    // Parse token_id
    let token_id = match U256::from_str(&trade.asset_id) {
        Ok(id) => id,
        Err(e) => {
            tracing::error!("Session {sid}: invalid asset_id '{}': {e}", trade.asset_id);
            return false;
        }
    };

    // Broadcast OrderPlaced
    let _ = update_tx.send(CopyTradeUpdate::OrderPlaced {
        session_id: sid.clone(),
        order: CopyTradeOrderSummary {
            id: order_id.to_string(),
            asset_id: trade.asset_id.clone(),
            side: trade.side.clone(),
            size_usdc: order_usdc,
            price: current_price,
            source_trader: trade.trader.clone(),
            simulate: false,
        },
        owner: session.config.owner.clone(),
    });

    // 8. EXECUTE — place CLOB order
    let clob = clob_client.read().await;
    let cs = match clob.as_ref() {
        Some(cs) => cs,
        None => {
            record_failed_order(
                order_id,
                &sid,
                trade,
                source_price,
                order_usdc,
                created_at,
                "CLOB client not initialized",
                session,
                user_db,
                update_tx,
            )
            .await;
            return false;
        }
    };

    let result = match order_type {
        CopyOrderType::FOK => {
            let usdc_dec = Decimal::from_f64_retain(order_usdc)
                .unwrap_or(Decimal::ZERO)
                .trunc_with_scale(6);
            let amount = match Amount::usdc(usdc_dec) {
                Ok(a) => a,
                Err(e) => {
                    record_failed_order(
                        order_id,
                        &sid,
                        trade,
                        source_price,
                        order_usdc,
                        created_at,
                        &format!("Invalid amount: {e}"),
                        session,
                        user_db,
                        update_tx,
                    )
                    .await;
                    return false;
                }
            };

            let signable = cs
                .client
                .market_order()
                .token_id(token_id)
                .side(side)
                .amount(amount)
                .order_type(OrderType::FOK)
                .build()
                .await;

            match signable {
                Ok(order) => match cs.client.sign(&cs.signer, order).await {
                    Ok(signed) => cs.client.post_order(signed).await,
                    Err(e) => Err(e),
                },
                Err(e) => Err(e),
            }
        }
        CopyOrderType::GTC => {
            let price_dec = Decimal::from_f64_retain(source_price)
                .unwrap_or(Decimal::ZERO)
                .trunc_with_scale(4);
            let shares = order_usdc / source_price;
            let size_dec = Decimal::from_f64_retain(shares)
                .unwrap_or(Decimal::ZERO)
                .trunc_with_scale(2);

            let signable = cs
                .client
                .limit_order()
                .token_id(token_id)
                .side(side)
                .price(price_dec)
                .size(size_dec)
                .order_type(OrderType::GTC)
                .build()
                .await;

            match signable {
                Ok(order) => match cs.client.sign(&cs.signer, order).await {
                    Ok(signed) => cs.client.post_order(signed).await,
                    Err(e) => Err(e),
                },
                Err(e) => Err(e),
            }
        }
    };

    // Drop the read lock
    drop(clob);

    // 9. RECORD + UPDATE CAPITAL
    match result {
        Ok(resp) if resp.success => {
            let fill_price_val;
            let status_str;
            let size_shares;
            let actual_slippage;

            match resp.status {
                OrderStatusType::Matched => {
                    // FOK filled — compute price per share (USDC/share)
                    fill_price_val = if resp.taking_amount > Decimal::ZERO
                        && resp.making_amount > Decimal::ZERO
                    {
                        let fp = match side {
                            // Buy: making=USDC sent, taking=shares received
                            Side::Buy => {
                                resp.making_amount.to_f64().unwrap_or(0.0)
                                    / resp.taking_amount.to_f64().unwrap_or(1.0)
                            }
                            // Sell: taking=USDC received, making=shares sent
                            _ => {
                                resp.taking_amount.to_f64().unwrap_or(0.0)
                                    / resp.making_amount.to_f64().unwrap_or(1.0)
                            }
                        };
                        Some(fp)
                    } else {
                        Some(current_price)
                    };
                    let shares_filled = match side {
                        Side::Buy => resp.taking_amount.to_f64().unwrap_or(0.0),
                        _ => resp.making_amount.to_f64().unwrap_or(0.0),
                    };
                    size_shares = Some(shares_filled);
                    actual_slippage = fill_price_val
                        .map(|fp| ((fp - source_price) / source_price * 10000.0).abs());
                    status_str = OrderStatus::Filled.as_str();
                    let fp = fill_price_val.unwrap_or(current_price);
                    // Position-aware capital tracking
                    match side {
                        Side::Buy => {
                            let usdc_spent = resp.making_amount.to_f64().unwrap_or(order_usdc);
                            session.remaining_capital -= usdc_spent;
                            let (cur_shares, _) = session
                                .positions
                                .get(&trade.asset_id)
                                .copied()
                                .unwrap_or((0.0, 0.0));
                            let new_shares = cur_shares + shares_filled;
                            session
                                .positions
                                .insert(trade.asset_id.clone(), (new_shares, fp));
                        }
                        _ => {
                            let usdc_received = resp.taking_amount.to_f64().unwrap_or(order_usdc);
                            session.remaining_capital += usdc_received;
                            let (cur_shares, _) = session
                                .positions
                                .get(&trade.asset_id)
                                .copied()
                                .unwrap_or((0.0, 0.0));
                            let new_shares = cur_shares - shares_filled;
                            if new_shares < 0.001 {
                                session.positions.remove(&trade.asset_id);
                            } else {
                                session
                                    .positions
                                    .insert(trade.asset_id.clone(), (new_shares, fp));
                            }
                        }
                    }
                }
                OrderStatusType::Live => {
                    // GTC resting
                    fill_price_val = None;
                    size_shares = Some(order_usdc / source_price);
                    actual_slippage = None;
                    status_str = OrderStatus::Submitted.as_str();
                    // Only deduct capital for buys (sells receive capital on fill)
                    if matches!(side, Side::Buy) {
                        session.remaining_capital -= order_usdc;
                    }
                    session.open_gtc_orders.insert(
                        resp.order_id.clone(),
                        (order_id.to_string(), Instant::now(), order_usdc),
                    );
                }
                OrderStatusType::Canceled | OrderStatusType::Unmatched => {
                    // FOK rejected — no fill
                    fill_price_val = None;
                    size_shares = None;
                    actual_slippage = None;
                    status_str = OrderStatus::Canceled.as_str();
                    // Do NOT deduct capital
                    tracing::warn!("Session {sid}: FOK order {} not filled", resp.order_id);
                }
                _ => {
                    fill_price_val = None;
                    size_shares = None;
                    actual_slippage = None;
                    status_str = OrderStatus::Submitted.as_str();
                }
            }

            let order_row = CopyTradeOrderRow {
                id: order_id.to_string(),
                session_id: sid.clone(),
                source_tx_hash: trade.tx_hash.clone(),
                source_trader: trade.trader.clone(),
                clob_order_id: Some(resp.order_id.clone()),
                asset_id: trade.asset_id.clone(),
                side: trade.side.clone(),
                price: current_price,
                source_price,
                size_usdc: order_usdc,
                size_shares,
                status: status_str.to_string(),
                error_message: None,
                fill_price: fill_price_val,
                slippage_bps: actual_slippage,
                tx_hash: resp.transaction_hashes.first().map(|h| h.to_string()),
                created_at: created_at.to_string(),
                updated_at: created_at.to_string(),
            };

            {
                let conn = user_db.lock().unwrap_or_else(|p| p.into_inner());
                let _ = db::insert_copytrade_order(&conn, &order_row);
            }

            tracing::info!(
                "Session {sid}: {status_str} {} {:.2} USDC on {} (CLOB order {})",
                trade.side,
                order_usdc,
                trade.asset_id,
                resp.order_id
            );

            if status_str == OrderStatus::Filled.as_str() {
                let _ = update_tx.send(CopyTradeUpdate::OrderFilled {
                    session_id: sid.clone(),
                    order_id: order_id.to_string(),
                    fill_price: fill_price_val.unwrap_or(current_price),
                    slippage_bps: actual_slippage.unwrap_or(0.0),
                    owner: session.config.owner.clone(),
                });
            }

            session.consecutive_failures = 0;
            true
        }
        Ok(resp) => {
            let error = resp
                .error_msg
                .unwrap_or_else(|| "Unknown CLOB error".into());
            record_failed_order(
                order_id,
                &sid,
                trade,
                source_price,
                order_usdc,
                created_at,
                &error,
                session,
                user_db,
                update_tx,
            )
            .await;
            false
        }
        Err(e) => {
            record_failed_order(
                order_id,
                &sid,
                trade,
                source_price,
                order_usdc,
                created_at,
                &e.to_string(),
                session,
                user_db,
                update_tx,
            )
            .await;
            false
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn fetch_clob_price(
    clob_client: &Arc<RwLock<Option<ClobClientState>>>,
    asset_id: &str,
    side: Side,
) -> Option<f64> {
    let token_id = U256::from_str(asset_id).ok()?;
    let clob = clob_client.read().await;
    let cs = clob.as_ref()?;
    let req = PriceRequest::builder()
        .token_id(token_id)
        .side(side)
        .build();
    let resp = cs.client.price(&req).await.ok()?;
    resp.price.to_f64()
}

use rust_decimal::prelude::ToPrimitive;

#[allow(clippy::too_many_arguments)]
async fn record_failed_order(
    order_id: &str,
    session_id: &str,
    trade: &LiveTrade,
    source_price: f64,
    order_usdc: f64,
    created_at: &str,
    error: &str,
    session: &mut ActiveSession,
    user_db: &Arc<Mutex<rusqlite::Connection>>,
    update_tx: &broadcast::Sender<CopyTradeUpdate>,
) {
    tracing::error!("Session {session_id}: order failed: {error}");

    let order_row = CopyTradeOrderRow {
        id: order_id.to_string(),
        session_id: session_id.to_string(),
        source_tx_hash: trade.tx_hash.clone(),
        source_trader: trade.trader.clone(),
        clob_order_id: None,
        asset_id: trade.asset_id.clone(),
        side: trade.side.clone(),
        price: source_price,
        source_price,
        size_usdc: order_usdc,
        size_shares: None,
        status: OrderStatus::Failed.as_str().to_string(),
        error_message: Some(error.to_string()),
        fill_price: None,
        slippage_bps: None,
        tx_hash: None,
        created_at: created_at.to_string(),
        updated_at: created_at.to_string(),
    };

    {
        let conn = user_db.lock().unwrap_or_else(|p| p.into_inner());
        let _ = db::insert_copytrade_order(&conn, &order_row);
    }

    let _ = update_tx.send(CopyTradeUpdate::OrderFailed {
        session_id: session_id.to_string(),
        order_id: order_id.to_string(),
        error: error.to_string(),
        owner: session.config.owner.clone(),
    });

    // Failure tracking
    session.consecutive_failures += 1;
    if session.consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
        session.cooldown_until = Some(Instant::now() + COOLDOWN_DURATION);
        tracing::warn!(
            "Session {session_id}: {} consecutive failures, entering {}s cooldown",
            session.consecutive_failures,
            COOLDOWN_DURATION.as_secs()
        );
    }
}

// ---------------------------------------------------------------------------
// Publish tracked addresses to ws_subscriber via watch channel
// ---------------------------------------------------------------------------

fn publish_tracked_addresses(
    sessions: &HashMap<String, ActiveSession>,
    trader_watch_tx: &tokio::sync::watch::Sender<std::collections::HashSet<String>>,
) {
    let union: std::collections::HashSet<String> = sessions
        .values()
        .filter(|s| SessionStatus::from_str(&s.config.status) == Some(SessionStatus::Running))
        .flat_map(|s| s.traders.iter().cloned())
        .map(|addr| addr.to_lowercase())
        .collect();

    tracing::info!(
        "Publishing {} tracked address(es) to ws_subscriber",
        union.len()
    );
    let _ = trader_watch_tx.send(union);
}

// ---------------------------------------------------------------------------
// Health check (60s interval)
// ---------------------------------------------------------------------------

async fn health_check(
    sessions: &mut HashMap<String, ActiveSession>,
    clob_client: &Arc<RwLock<Option<ClobClientState>>>,
    user_db: &Arc<Mutex<rusqlite::Connection>>,
    update_tx: &broadcast::Sender<CopyTradeUpdate>,
    trader_watch_tx: &tokio::sync::watch::Sender<std::collections::HashSet<String>>,
) {
    let mut to_stop: Vec<(String, String, String)> = Vec::new(); // (id, owner, reason)

    for (sid, session) in sessions.iter_mut() {
        // Sync remaining_capital to SQLite
        {
            let conn = user_db.lock().unwrap_or_else(|p| p.into_inner());
            let _ = db::update_session_capital(&conn, sid, session.remaining_capital);
        }

        // Circuit breaker — account for unrealized value in open positions
        if let Some(max_loss_pct) = session.config.max_loss_pct {
            // Unrealized value = sum(shares * last_fill_price)
            // Uses the most recent fill price per asset as best available estimate
            let unrealized_value: f64 = session
                .positions
                .values()
                .map(|(shares, last_price)| shares * last_price)
                .sum();
            let total_value = session.remaining_capital + unrealized_value;
            let pnl = total_value - session.config.initial_capital;
            let loss_pct = -pnl / session.config.initial_capital * 100.0;
            if loss_pct > max_loss_pct {
                tracing::error!(
                    "Session {sid} auto-stopped: loss {loss_pct:.1}% exceeds max {max_loss_pct:.1}% (cash={:.2}, positions={:.2})",
                    session.remaining_capital,
                    unrealized_value
                );
                to_stop.push((
                    sid.clone(),
                    session.config.owner.clone(),
                    "circuit_breaker".to_string(),
                ));
                continue;
            }
        }

        // Cancel GTC orders older than 1 hour
        let expired: Vec<String> = session
            .open_gtc_orders
            .iter()
            .filter(|(_, (_, placed_at, _))| placed_at.elapsed() > GTC_TIMEOUT)
            .map(|(clob_id, _)| clob_id.clone())
            .collect();

        if !expired.is_empty() {
            // Fetch cancel result, then drop the async lock before acquiring mutex
            let cancel_result = {
                let clob = clob_client.read().await;
                if let Some(ref cs) = *clob {
                    let ids: Vec<&str> = expired.iter().map(|s| s.as_str()).collect();
                    Some(cs.client.cancel_orders(&ids).await)
                } else {
                    None
                }
            }; // clob read guard dropped here

            if let Some(Ok(resp)) = cancel_result {
                for canceled_id in &resp.canceled {
                    if let Some((our_id, _, usdc)) = session.open_gtc_orders.remove(canceled_id) {
                        session.remaining_capital += usdc; // Refund capital
                        let conn = user_db.lock().unwrap_or_else(|p| p.into_inner());
                        let _ = db::update_copytrade_order(
                            &conn, &our_id, "canceled", None, None, None, None,
                        );
                    }
                }
                tracing::info!(
                    "Canceled {} expired GTC orders for session {sid}",
                    resp.canceled.len()
                );
            } else if let Some(Err(e)) = cancel_result {
                tracing::warn!("Failed to cancel expired GTC orders: {e}");
            }
        }
    }

    // Process stops outside the mutable borrow
    let had_stops = !to_stop.is_empty();
    for (sid, owner, reason) in to_stop {
        if let Some(session) = sessions.remove(&sid) {
            // Cancel remaining GTC orders
            if !session.open_gtc_orders.is_empty() {
                let clob = clob_client.read().await;
                if let Some(ref cs) = *clob {
                    let ids: Vec<&str> =
                        session.open_gtc_orders.keys().map(|s| s.as_str()).collect();
                    let _ = cs.client.cancel_orders(&ids).await;
                }
            }
            let conn = user_db.lock().unwrap_or_else(|p| p.into_inner());
            let _ = db::update_session_status(&conn, &sid, "stopped");
            let _ = update_tx.send(CopyTradeUpdate::SessionStopped {
                session_id: sid,
                reason: Some(reason),
                owner,
            });
        }
    }

    if had_stops {
        publish_tracked_addresses(sessions, trader_watch_tx);
    }
}
