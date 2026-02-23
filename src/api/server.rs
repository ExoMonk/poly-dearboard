use axum::routing::{delete, get, post};
use axum::Router;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, RwLock};
use tower_http::cors::{Any, CorsLayer};

use super::{alerts, contracts, copytrade, db, engine, markets, routes, scanner, wallet, ws_subscriber, types::LeaderboardResponse};

/// Cached leaderboard response with expiry.
pub struct CachedResponse {
    pub data: LeaderboardResponse,
    pub expires: std::time::Instant,
}

pub type LeaderboardCache = Arc<RwLock<HashMap<String, CachedResponse>>>;

/// Per-wallet balance + approval state (ephemeral, not persisted).
#[derive(Clone)]
pub struct WalletBalanceState {
    pub usdc_balance: String,
    pub usdc_raw: String,
    pub pol_balance: String,
    pub pol_raw: String,
    pub ctf_approved: bool,
    pub neg_risk_approved: bool,
    pub last_checked: std::time::Instant,
}

pub type WalletBalances = Arc<RwLock<HashMap<String, WalletBalanceState>>>;

#[derive(Clone)]
pub struct AppState {
    pub db: clickhouse::Client,
    pub http: reqwest::Client,
    pub market_cache: markets::MarketCache,
    pub alert_tx: broadcast::Sender<alerts::Alert>,
    pub trade_tx: broadcast::Sender<alerts::LiveTrade>,
    pub metadata_tx: tokio::sync::mpsc::Sender<(String, markets::MarketInfo)>,
    pub leaderboard_cache: LeaderboardCache,
    pub user_db: Arc<Mutex<rusqlite::Connection>>,
    pub jwt_secret: Arc<Vec<u8>>,
    pub copytrade_live_tx: broadcast::Sender<alerts::LiveTrade>,
    pub trader_watch_tx: tokio::sync::watch::Sender<HashSet<String>>,
    pub encryption_key: Arc<[u8; 32]>,
    pub erpc_url: Arc<String>,
    pub wallet_balances: WalletBalances,
    pub copytrade_cmd_tx: tokio::sync::mpsc::Sender<engine::CopyTradeCommand>,
    pub copytrade_update_tx: broadcast::Sender<super::types::CopyTradeUpdate>,
    pub clob_client: Arc<RwLock<Option<engine::ClobClientState>>>,
}

async fn metadata_writer(
    db: clickhouse::Client,
    mut rx: tokio::sync::mpsc::Receiver<(String, markets::MarketInfo)>,
) {
    use super::types::MarketMetadataRow;

    let mut batch: Vec<MarketMetadataRow> = Vec::with_capacity(100);
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));

    loop {
        tokio::select! {
            Some((asset_id, info)) = rx.recv() => {
                let now = chrono::Utc::now().timestamp() as u32;
                batch.push(MarketMetadataRow {
                    asset_id,
                    question: info.question,
                    outcome: info.outcome,
                    category: info.category,
                    condition_id: info.condition_id.unwrap_or_default(),
                    gamma_token_id: info.gamma_token_id,
                    outcome_index: info.outcome_index as u8,
                    active: if info.active { 1 } else { 0 },
                    all_token_ids: info.all_token_ids,
                    outcomes: info.outcomes,
                    updated_at: now,
                });
                if batch.len() >= 100 {
                    flush_metadata_batch(&db, &mut batch).await;
                }
            }
            _ = interval.tick() => {
                if !batch.is_empty() {
                    flush_metadata_batch(&db, &mut batch).await;
                }
            }
        }
    }
}

async fn flush_metadata_batch(
    db: &clickhouse::Client,
    batch: &mut Vec<super::types::MarketMetadataRow>,
) {
    let mut inserter = match db.insert("poly_dearboard.market_metadata") {
        Ok(i) => i,
        Err(e) => {
            tracing::warn!("market_metadata batch insert failed: {e}");
            batch.clear();
            return;
        }
    };
    let rows: Vec<_> = batch.drain(..).collect();
    for row in rows {
        if let Err(e) = inserter.write(&row).await {
            tracing::warn!("market_metadata row write failed: {e}");
            return;
        }
    }
    if let Err(e) = inserter.end().await {
        tracing::warn!("market_metadata batch flush failed: {e}");
    }
}

pub async fn run(client: clickhouse::Client, port: u16) {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let jwt_secret = std::env::var("JWT_SECRET")
        .expect("JWT_SECRET env var is required for wallet authentication");

    let encryption_key_hex = std::env::var("WALLET_ENCRYPTION_KEY")
        .expect("WALLET_ENCRYPTION_KEY env var is required (64 hex chars = 32 bytes)");
    let encryption_key_bytes = hex::decode(encryption_key_hex.trim())
        .expect("WALLET_ENCRYPTION_KEY must be valid hex");
    let encryption_key: [u8; 32] = encryption_key_bytes
        .try_into()
        .expect("WALLET_ENCRYPTION_KEY must be exactly 32 bytes (64 hex chars)");

    let erpc_url = std::env::var("POLYGON_RPC_URL")
        .unwrap_or_else(|_| "http://localhost:4000/main/evm/137".into());

    let user_conn = db::init_user_db("data/users.db");

    let (alert_tx, _) = broadcast::channel::<alerts::Alert>(256);
    let (trade_tx, _) = broadcast::channel::<alerts::LiveTrade>(512);
    let (metadata_tx, metadata_rx) =
        tokio::sync::mpsc::channel::<(String, markets::MarketInfo)>(1024);
    let (copytrade_cmd_tx, copytrade_cmd_rx) =
        tokio::sync::mpsc::channel::<engine::CopyTradeCommand>(64);
    let (copytrade_update_tx, _) =
        broadcast::channel::<super::types::CopyTradeUpdate>(256);
    let (copytrade_live_tx, _) = broadcast::channel::<alerts::LiveTrade>(128);
    let (trader_watch_tx, trader_watch_rx) =
        tokio::sync::watch::channel::<HashSet<String>>(HashSet::new());

    let state = AppState {
        db: client,
        http: reqwest::Client::new(),
        market_cache: markets::new_cache(),
        alert_tx,
        trade_tx,
        metadata_tx,
        leaderboard_cache: Arc::new(RwLock::new(HashMap::new())),
        user_db: Arc::new(Mutex::new(user_conn)),
        jwt_secret: Arc::new(jwt_secret.into_bytes()),
        copytrade_live_tx,
        trader_watch_tx,
        encryption_key: Arc::new(encryption_key),
        erpc_url: Arc::new(erpc_url),
        wallet_balances: Arc::new(RwLock::new(HashMap::new())),
        copytrade_cmd_tx,
        copytrade_update_tx,
        clob_client: Arc::new(RwLock::new(None)),
    };

    // Pre-warm the market name cache in the background, then refresh periodically
    {
        let http = state.http.clone();
        let db = state.db.clone();
        let cache = state.market_cache.clone();
        tokio::spawn(async move {
            markets::warm_cache(&http, &db, &cache).await;
            markets::persist_cache_to_clickhouse(&db, &cache).await;
            markets::populate_resolved_prices(&db, &cache).await;
            // Re-warm every 10 minutes to catch new markets + resolutions
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(600));
            interval.tick().await; // skip immediate tick
            loop {
                interval.tick().await;
                tracing::info!("Refreshing market cache...");
                markets::warm_cache(&http, &db, &cache).await;
                markets::persist_cache_to_clickhouse(&db, &cache).await;
                markets::populate_resolved_prices(&db, &cache).await;
            }
        });
    }

    // Batched metadata writer: drains webhook-time metadata inserts into ClickHouse
    {
        let db = state.db.clone();
        tokio::spawn(metadata_writer(db, metadata_rx));
    }

    // Background leaderboard cache warmer — keeps the default view always warm
    {
        let state = state.clone();
        tokio::spawn(async move {
            // Wait for market cache to warm first
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            loop {
                let _ = routes::warm_leaderboard(&state).await;
                tokio::time::sleep(std::time::Duration::from_secs(25)).await;
            }
        });
    }

    // Phantom fill scanner: polls Polygon blocks for reverted exchange TXs
    {
        let rpc_url = std::env::var("POLYGON_RPC_URL")
            .unwrap_or_else(|_| "http://erpc:4000/main/evm/137".into());
        let http = state.http.clone();
        let alert_tx = state.alert_tx.clone();
        tokio::spawn(scanner::run(http, rpc_url, alert_tx));
    }

    // Balance polling: checks USDC.e balance + allowances for all trading wallets
    {
        let state = state.clone();
        tokio::spawn(balance_poll_task(state));
    }

    // Copy-trade engine: subscribes to copytrade_live_tx (targeted WS trades), places CLOB orders
    {
        let trade_rx = state.copytrade_live_tx.subscribe();
        let update_tx = state.copytrade_update_tx.clone();
        let clob = state.clob_client.clone();
        let udb = state.user_db.clone();
        let enc = state.encryption_key.clone();
        let ch = state.db.clone();
        let watch_tx = state.trader_watch_tx.clone();
        tokio::spawn(engine::copytrade_engine_loop(
            trade_rx, copytrade_cmd_rx, update_tx, clob, udb, enc, ch, watch_tx,
        ));
    }

    // Targeted eth_subscribe for copy-trade sessions only (zero CU when no sessions active)
    {
        let copytrade_tx = state.copytrade_live_tx.clone();
        let cache = state.market_cache.clone();
        let http = state.http.clone();
        let rpc_url = std::env::var("POLYGON_RPC_URL")
            .unwrap_or_else(|_| "http://erpc:4000/main/evm/137".into());
        tokio::spawn(ws_subscriber::run(copytrade_tx, trader_watch_rx, cache, http, rpc_url));
    }

    // Public API routes (no auth required)
    let public_api = Router::new()
        .route("/auth/nonce", get(routes::auth_nonce))
        .route("/auth/verify", post(routes::auth_verify))
        .route("/health", get(routes::health));

    // Protected API routes (JWT required — AuthUser extractor on each handler)
    let protected_api = Router::new()
        .route("/leaderboard", get(routes::leaderboard))
        .route("/trader/{address}", get(routes::trader_stats))
        .route("/trader/{address}/trades", get(routes::trader_trades))
        .route("/trader/{address}/positions", get(routes::trader_positions))
        .route("/trader/{address}/pnl-chart", get(routes::pnl_chart))
        .route("/markets/hot", get(routes::hot_markets))
        .route("/trades/recent", get(routes::recent_trades))
        .route("/market/resolve", get(routes::resolve_market))
        .route("/smart-money", get(routes::smart_money))
        .route("/trader/{address}/profile", get(routes::trader_profile))
        .route("/lab/backtest", post(routes::backtest))
        .route("/lab/copy-portfolio", get(routes::copy_portfolio))
        // Trader Lists CRUD
        .route("/lists", get(routes::list_trader_lists).post(routes::create_trader_list))
        .route("/lists/{id}", get(routes::get_trader_list).patch(routes::rename_trader_list).delete(routes::delete_trader_list))
        .route("/lists/{id}/members", post(routes::add_list_members).delete(routes::remove_list_members))
        // Trading Wallets (multi-wallet, up to 3 per user)
        .route("/wallets", get(wallet::get_wallets))
        .route("/wallets/generate", post(wallet::generate_wallet))
        .route("/wallets/import", post(wallet::import_wallet))
        .route("/wallets/{id}/derive-credentials", post(wallet::derive_credentials))
        .route("/wallets/{id}/balance", get(wallet::get_balance))
        .route("/wallets/{id}/approve", post(wallet::approve_exchanges))
        .route("/wallets/{id}/deposit-address", get(wallet::get_deposit_address))
        .route("/wallets/{id}/deposit-status", get(wallet::get_deposit_status))
        .route("/wallets/{id}", delete(wallet::delete_wallet))
        // Copy-Trade Engine
        .route("/copytrade/sessions", get(copytrade::list_sessions).post(copytrade::create_session))
        .route("/copytrade/sessions/{id}", get(copytrade::get_session).patch(copytrade::update_session).delete(copytrade::delete_session))
        .route("/copytrade/sessions/{id}/orders", get(copytrade::list_session_orders))
        .route("/copytrade/sessions/{id}/stats", get(copytrade::get_session_stats))
        .route("/copytrade/sessions/{id}/positions", get(copytrade::get_session_positions))
        .route("/copytrade/summary", get(copytrade::get_summary))
        .route("/copytrade/active-traders", get(copytrade::get_active_traders))
        .route("/copytrade/close-position", post(copytrade::close_position));

    let app = Router::new()
        .nest("/api", public_api.merge(protected_api))
        .route("/webhooks/rindexer", post(alerts::webhook_handler))
        .route("/ws/alerts", get(alerts::ws_handler))
        .route("/ws/trades", get(alerts::trades_ws_handler))
        // Signal feed WS (auth handled via query param in handler)
        .route("/ws/signals", get(alerts::signals_ws_handler))
        // Copy-trade updates WS
        .route("/ws/copytrade", get(alerts::copytrade_ws_handler))
        .layer(cors)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .expect("Failed to bind");

    tracing::info!("API server listening on port {port}");
    axum::serve(listener, app).await.expect("Server failed");
}

/// Background task: polls USDC.e balance + allowances for all trading wallets every 30s.
async fn balance_poll_task(state: AppState) {
    use alloy::primitives::Address;
    use alloy::providers::Provider;

    // Wait for eRPC and other services to be ready
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        interval.tick().await;

        // Collect all wallet addresses + IDs from SQLite
        // proxy_address holds USDC.e + allowances; wallet_address (EOA) holds POL for gas
        let wallets = {
            let state = state.clone();
            match tokio::task::spawn_blocking(move || {
                let conn = state.user_db.lock().expect("user_db lock");
                let mut stmt = conn
                    .prepare("SELECT id, wallet_address, proxy_address FROM trading_wallets")
                    .ok()?;
                let rows: Vec<(String, String, Option<String>)> = stmt
                    .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
                    .ok()?
                    .filter_map(|r| r.ok())
                    .collect();
                Some(rows)
            })
            .await
            {
                Ok(Some(w)) if !w.is_empty() => w,
                _ => continue,
            }
        };

        let provider = contracts::create_provider(&state.erpc_url);
        let usdc = contracts::IERC20::new(contracts::USDC_ADDRESS, &provider);

        for (wallet_id, eoa_str, proxy_str) in &wallets {
            let eoa = match eoa_str.parse::<Address>() {
                Ok(a) => a,
                Err(_) => continue,
            };
            // USDC.e balance lives on the proxy; allowances + POL on the EOA
            let proxy = proxy_str
                .as_deref()
                .and_then(|s| s.parse::<Address>().ok())
                .unwrap_or(eoa);

            let bal_call = usdc.balanceOf(proxy);
            let ctf_call = usdc.allowance(eoa, contracts::CTF_EXCHANGE);
            let neg_call = usdc.allowance(eoa, contracts::NEG_RISK_EXCHANGE);
            let (balance_res, ctf_allow_res, neg_allow_res, pol_gas_res) = tokio::join!(
                bal_call.call(),
                ctf_call.call(),
                neg_call.call(),
                provider.get_balance(eoa),
            );

            let usdc_raw = match balance_res {
                Ok(raw) => raw,
                Err(e) => {
                    tracing::error!("Balance poll failed for {eoa_str}: {e}");
                    continue;
                }
            };
            let ctf_allowance = ctf_allow_res.inspect_err(|e| {
                tracing::error!("CTF allowance poll failed for {eoa_str}: {e}");
            }).unwrap_or_default();
            let neg_allowance = neg_allow_res.inspect_err(|e| {
                tracing::error!("NegRisk allowance poll failed for {eoa_str}: {e}");
            }).unwrap_or_default();
            let pol_wei = pol_gas_res.unwrap_or_default();

            if usdc_raw > alloy::primitives::U256::ZERO && usdc_raw < contracts::LOW_BALANCE_RAW {
                tracing::warn!(
                    "Low USDC.e balance for wallet {eoa_str}: {}",
                    contracts::format_usdc(usdc_raw)
                );
            }

            let entry = WalletBalanceState {
                usdc_balance: contracts::format_usdc(usdc_raw),
                usdc_raw: usdc_raw.to_string(),
                pol_balance: contracts::format_pol(pol_wei),
                pol_raw: pol_wei.to_string(),
                ctf_approved: !ctf_allowance.is_zero(),
                neg_risk_approved: !neg_allowance.is_zero(),
                last_checked: std::time::Instant::now(),
            };

            state.wallet_balances.write().await.insert(wallet_id.clone(), entry);
        }
    }
}
