use axum::{routing::{get, post}, Router};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tower_http::cors::{Any, CorsLayer};

use super::{alerts, markets, routes, scanner, types::LeaderboardResponse};

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
    pub leaderboard_cache: LeaderboardCache,
}

pub async fn run(client: clickhouse::Client, port: u16) {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let (alert_tx, _) = broadcast::channel::<alerts::Alert>(256);
    let (trade_tx, _) = broadcast::channel::<alerts::LiveTrade>(512);

    let state = AppState {
        db: client,
        http: reqwest::Client::new(),
        market_cache: markets::new_cache(),
        alert_tx,
        trade_tx,
        leaderboard_cache: Arc::new(RwLock::new(HashMap::new())),
    };

    // Pre-warm the market name cache in the background, then refresh periodically
    {
        let http = state.http.clone();
        let db = state.db.clone();
        let cache = state.market_cache.clone();
        tokio::spawn(async move {
            markets::warm_cache(&http, &db, &cache).await;
            markets::populate_resolved_prices(&db, &cache).await;
            // Re-warm every 10 minutes to catch new markets + resolutions
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(600));
            interval.tick().await; // skip immediate tick
            loop {
                interval.tick().await;
                tracing::info!("Refreshing market cache...");
                markets::warm_cache(&http, &db, &cache).await;
                markets::populate_resolved_prices(&db, &cache).await;
            }
        });
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

    let api = Router::new()
        .route("/leaderboard", get(routes::leaderboard))
        .route("/trader/{address}", get(routes::trader_stats))
        .route("/trader/{address}/trades", get(routes::trader_trades))
        .route("/trader/{address}/positions", get(routes::trader_positions))
        .route("/trader/{address}/pnl-chart", get(routes::pnl_chart))
        .route("/markets/hot", get(routes::hot_markets))
        .route("/trades/recent", get(routes::recent_trades))
        .route("/health", get(routes::health))
        .route("/market/resolve", get(routes::resolve_market))
        .route("/auth/verify", post(routes::verify_access_code))
        .route("/smart-money", get(routes::smart_money))
        .route("/trader/{address}/profile", get(routes::trader_profile));

    let app = Router::new()
        .nest("/api", api)
        .route("/webhooks/rindexer", post(alerts::webhook_handler))
        .route("/ws/alerts", get(alerts::ws_handler))
        .route("/ws/trades", get(alerts::trades_ws_handler))
        .layer(cors)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .expect("Failed to bind");

    tracing::info!("API server listening on port {port}");
    axum::serve(listener, app).await.expect("Server failed");
}
