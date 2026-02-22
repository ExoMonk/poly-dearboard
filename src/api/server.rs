use axum::{routing::{get, post}, Router};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, RwLock};
use tower_http::cors::{Any, CorsLayer};

use super::{alerts, db, markets, routes, scanner, types::LeaderboardResponse};

/// Cached leaderboard response with expiry.
pub struct CachedResponse {
    pub data: LeaderboardResponse,
    pub expires: std::time::Instant,
}

pub type LeaderboardCache = Arc<RwLock<HashMap<String, CachedResponse>>>;

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

    let user_conn = db::init_user_db("data/users.db");

    let (alert_tx, _) = broadcast::channel::<alerts::Alert>(256);
    let (trade_tx, _) = broadcast::channel::<alerts::LiveTrade>(512);
    let (metadata_tx, metadata_rx) =
        tokio::sync::mpsc::channel::<(String, markets::MarketInfo)>(1024);

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
        .route("/lists/{id}/members", post(routes::add_list_members).delete(routes::remove_list_members));

    let app = Router::new()
        .nest("/api", public_api.merge(protected_api))
        .route("/webhooks/rindexer", post(alerts::webhook_handler))
        .route("/ws/alerts", get(alerts::ws_handler))
        .route("/ws/trades", get(alerts::trades_ws_handler))
        // Signal feed WS (auth handled via query param in handler)
        .route("/ws/signals", get(alerts::signals_ws_handler))
        .layer(cors)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .expect("Failed to bind");

    tracing::info!("API server listening on port {port}");
    axum::serve(listener, app).await.expect("Server failed");
}
