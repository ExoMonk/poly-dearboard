use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};

use serde::Deserialize;

use super::middleware::AuthUser;
use super::server::AppState;
use super::types::*;
use super::{db, markets, middleware};

const ALLOWED_SORT_COLUMNS: &[&str] = &["realized_pnl", "total_volume", "trade_count"];

/// Exchange contracts that appear as `maker` in taker-summary OrderFilled events.
/// These are protocol intermediaries, not real traders. Safety net filter —
/// with maker-only MVs the exchange should never appear as trader, but keep
/// this in case of edge cases or future schema changes.
const EXCHANGE_CONTRACTS: &[&str] = &[
    "0x4bFb41d5B3570DeFd03C39a9A4D8dE6Bd8B8982E", // CTF Exchange
    "0xC5d563A36AE78145C45a50134d48A1215220f80a", // NegRisk CTF Exchange
    "0x02A86f51aA7B8b1c17c30364748d5Ae4a0727E23", // Polymarket Relayer
];

pub(crate) fn exclude_clause() -> String {
    EXCHANGE_CONTRACTS
        .iter()
        .map(|a| format!("'{a}'"))
        .collect::<Vec<_>>()
        .join(",")
}

/// Background cache warmer — runs the default leaderboard query and populates the cache.
pub async fn warm_leaderboard(state: &AppState) -> Result<(), String> {
    let sort = "realized_pnl";
    let order = "desc";
    let limit: u32 = 25;
    let offset: u32 = 0;
    let timeframe = "all";
    let cache_key = format!("{sort}:{order}:{limit}:{offset}:{timeframe}");

    let exclude = exclude_clause();
    let sort_expr = "sum((p.sell_usdc - p.buy_usdc) + (p.buy_amount - p.sell_amount) * coalesce(rp.resolved_price, toFloat64(lp.latest_price)))";

    let query = format!(
        "WITH resolved AS (
            SELECT asset_id, toNullable(toFloat64(resolved_price)) AS resolved_price
            FROM poly_dearboard.resolved_prices FINAL
        )
        SELECT
            toString(p.trader) AS address,
            toString(sum(p.total_volume)) AS total_volume,
            sum(p.trade_count) AS trade_count,
            count() AS markets_traded,
            toString(ROUND(sum((p.sell_usdc - p.buy_usdc) + (p.buy_amount - p.sell_amount) * coalesce(rp.resolved_price, toFloat64(lp.latest_price))), 6)) AS realized_pnl,
            toString(sum(p.total_fee)) AS total_fees,
            ifNull(toString(min(p.first_ts)), '') AS first_trade,
            ifNull(toString(max(p.last_ts)), '') AS last_trade
        FROM poly_dearboard.trader_positions p
        LEFT JOIN (SELECT asset_id, latest_price FROM poly_dearboard.asset_latest_price FINAL) AS lp ON p.asset_id = lp.asset_id
        LEFT JOIN resolved rp ON p.asset_id = rp.asset_id
        WHERE p.trader NOT IN ({exclude})
        GROUP BY p.trader
        ORDER BY {sort_expr} {order}
        LIMIT ? OFFSET ?"
    );

    let traders = state
        .db
        .query(&query)
        .bind(limit)
        .bind(offset)
        .fetch_all::<TraderSummary>()
        .await
        .map_err(|e| e.to_string())?;

    let total: u64 = state
        .db
        .query("SELECT uniqExactMerge(unique_traders) FROM poly_dearboard.global_stats")
        .fetch_one()
        .await
        .map_err(|e| e.to_string())?;

    let addresses: Vec<String> = traders.iter().map(|t| t.address.to_lowercase()).collect();
    let (labels, label_details) = match tokio::time::timeout(
        std::time::Duration::from_secs(2),
        batch_compute_labels(state, &addresses),
    )
    .await
    {
        Ok(pair) => pair,
        Err(_) => (
            std::collections::HashMap::new(),
            std::collections::HashMap::new(),
        ),
    };

    let response = LeaderboardResponse {
        traders,
        total,
        limit,
        offset,
        labels,
        label_details,
    };

    let mut cache = state.leaderboard_cache.write().await;
    cache.insert(
        cache_key,
        super::server::CachedResponse {
            data: response,
            expires: std::time::Instant::now() + std::time::Duration::from_secs(30),
        },
    );

    tracing::debug!("leaderboard cache warmed");
    Ok(())
}

pub async fn leaderboard(
    State(state): State<AppState>,
    Query(params): Query<LeaderboardParams>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let sort = params.sort.as_deref().unwrap_or("realized_pnl");
    let order = params.order.as_deref().unwrap_or("desc");
    let limit = params.limit.unwrap_or(100).min(500);
    let offset = params.offset.unwrap_or(0);
    let timeframe = params.timeframe.as_deref().unwrap_or("all");

    // Check cache (30s TTL)
    let cache_key = format!("{sort}:{order}:{limit}:{offset}:{timeframe}");
    {
        let cache = state.leaderboard_cache.read().await;
        if let Some(entry) = cache.get(&cache_key) {
            if entry.expires > std::time::Instant::now() {
                tracing::info!("leaderboard: cache hit ({cache_key})");
                return Ok(Json(entry.data.clone()));
            }
        }
    }

    if !ALLOWED_SORT_COLUMNS.contains(&sort) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Invalid sort column. Allowed: {ALLOWED_SORT_COLUMNS:?}"),
        ));
    }
    if order != "asc" && order != "desc" {
        return Err((
            StatusCode::BAD_REQUEST,
            "Invalid order. Allowed: asc, desc".into(),
        ));
    }

    let exclude = exclude_clause();

    let (traders, total) = if timeframe == "all" {
        // All-time: read from pre-aggregated trader_positions table
        let sort_expr = match sort {
            "realized_pnl" => {
                "sum((p.sell_usdc - p.buy_usdc) + (p.buy_amount - p.sell_amount) * coalesce(rp.resolved_price, toFloat64(lp.latest_price)))"
            }
            "total_volume" => "sum(p.total_volume)",
            "trade_count" => "sum(p.trade_count)",
            _ => unreachable!(),
        };

        let query = format!(
            "WITH resolved AS (
                SELECT asset_id, toNullable(toFloat64(resolved_price)) AS resolved_price
                FROM poly_dearboard.resolved_prices FINAL
            )
            SELECT
                toString(p.trader) AS address,
                toString(sum(p.total_volume)) AS total_volume,
                sum(p.trade_count) AS trade_count,
                count() AS markets_traded,
                toString(ROUND(sum((p.sell_usdc - p.buy_usdc) + (p.buy_amount - p.sell_amount) * coalesce(rp.resolved_price, toFloat64(lp.latest_price))), 6)) AS realized_pnl,
                toString(sum(p.total_fee)) AS total_fees,
                ifNull(toString(min(p.first_ts)), '') AS first_trade,
                ifNull(toString(max(p.last_ts)), '') AS last_trade
            FROM poly_dearboard.trader_positions p
            LEFT JOIN (SELECT asset_id, latest_price FROM poly_dearboard.asset_latest_price FINAL) AS lp ON p.asset_id = lp.asset_id
            LEFT JOIN resolved rp ON p.asset_id = rp.asset_id
            WHERE p.trader NOT IN ({exclude})
            GROUP BY p.trader
            ORDER BY {sort_expr} {order}
            LIMIT ? OFFSET ?"
        );

        let traders = state
            .db
            .query(&query)
            .bind(limit)
            .bind(offset)
            .fetch_all::<TraderSummary>()
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        let total: u64 = state
            .db
            .query("SELECT uniqExactMerge(unique_traders) FROM poly_dearboard.global_stats")
            .fetch_one()
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        (traders, total)
    } else {
        // Time-windowed (1h/24h): read from raw trades (within TTL) + asset_latest_price
        let prewhere = match timeframe {
            "1h" => "PREWHERE block_timestamp >= now() - INTERVAL 1 HOUR",
            "24h" => "PREWHERE block_timestamp >= now() - INTERVAL 24 HOUR",
            _ => "",
        };

        let sort_expr = match sort {
            "realized_pnl" => {
                "sum(p.cash_flow + p.net_tokens * coalesce(rp.resolved_price, toFloat64(lp.latest_price)))"
            }
            "total_volume" => "sum(p.volume)",
            "trade_count" => "sum(p.trades)",
            _ => unreachable!(),
        };

        let query = format!(
            "WITH
                resolved AS (
                    SELECT asset_id, toNullable(toFloat64(resolved_price)) AS resolved_price
                    FROM poly_dearboard.resolved_prices FINAL
                ),
                positions AS (
                    SELECT trader, asset_id,
                           sumIf(amount, side = 'buy') - sumIf(amount, side = 'sell') AS net_tokens,
                           sumIf(usdc_amount, side = 'sell') - sumIf(usdc_amount, side = 'buy') AS cash_flow,
                           sum(usdc_amount) AS volume,
                           count() AS trades,
                           sum(fee) AS fees,
                           min(if(block_timestamp = toDateTime('1970-01-01 00:00:00'), NULL, block_timestamp)) AS first_ts,
                           max(if(block_timestamp = toDateTime('1970-01-01 00:00:00'), NULL, block_timestamp)) AS last_ts
                    FROM poly_dearboard.trades
                    {prewhere}
                    WHERE trader NOT IN ({exclude})
                    GROUP BY trader, asset_id
                )
            SELECT
                toString(p.trader) AS address,
                toString(sum(p.volume)) AS total_volume,
                sum(p.trades) AS trade_count,
                count() AS markets_traded,
                toString(ROUND(sum(p.cash_flow + p.net_tokens * coalesce(rp.resolved_price, toFloat64(lp.latest_price))), 6)) AS realized_pnl,
                toString(sum(p.fees)) AS total_fees,
                ifNull(toString(min(p.first_ts)), '') AS first_trade,
                ifNull(toString(max(p.last_ts)), '') AS last_trade
            FROM positions p
            LEFT JOIN (SELECT asset_id, latest_price FROM poly_dearboard.asset_latest_price FINAL) AS lp ON p.asset_id = lp.asset_id
            LEFT JOIN resolved rp ON p.asset_id = rp.asset_id
            GROUP BY p.trader
            ORDER BY {sort_expr} {order}
            LIMIT ? OFFSET ?"
        );

        let traders = state
            .db
            .query(&query)
            .bind(limit)
            .bind(offset)
            .fetch_all::<TraderSummary>()
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        let total: u64 = state
            .db
            .query(&format!(
                "SELECT uniqExact(trader) FROM poly_dearboard.trades {prewhere} WHERE trader NOT IN ({exclude})"
            ))
            .fetch_one()
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        (traders, total)
    };

    // Batch-compute labels for the current page of traders (with timeout)
    let addresses: Vec<String> = traders.iter().map(|t| t.address.to_lowercase()).collect();
    let (labels, label_details) = match tokio::time::timeout(
        std::time::Duration::from_secs(2),
        batch_compute_labels(&state, &addresses),
    )
    .await
    {
        Ok(pair) => pair,
        Err(_) => {
            tracing::warn!("batch_compute_labels timed out after 2s");
            (
                std::collections::HashMap::new(),
                std::collections::HashMap::new(),
            )
        }
    };

    let response = LeaderboardResponse {
        traders,
        total,
        limit,
        offset,
        labels,
        label_details,
    };

    // Cache for 30 seconds
    {
        let mut cache = state.leaderboard_cache.write().await;
        cache.insert(
            cache_key,
            super::server::CachedResponse {
                data: response.clone(),
                expires: std::time::Instant::now() + std::time::Duration::from_secs(30),
            },
        );
    }

    Ok(Json(response))
}

pub async fn trader_stats(
    State(state): State<AppState>,
    Path(address): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let address = address.to_lowercase();

    let result = state
        .db
        .query(
            "WITH resolved AS (
                SELECT asset_id, toNullable(toFloat64(resolved_price)) AS resolved_price
                FROM poly_dearboard.resolved_prices FINAL
            )
            SELECT
                toString(p.trader) AS address,
                toString(sum(p.total_volume)) AS total_volume,
                sum(p.trade_count) AS trade_count,
                count() AS markets_traded,
                toString(ROUND(sum((p.sell_usdc - p.buy_usdc) + (p.buy_amount - p.sell_amount) * coalesce(rp.resolved_price, toFloat64(lp.latest_price))), 6)) AS realized_pnl,
                toString(sum(p.total_fee)) AS total_fees,
                ifNull(toString(min(p.first_ts)), '') AS first_trade,
                ifNull(toString(max(p.last_ts)), '') AS last_trade
            FROM poly_dearboard.trader_positions p
            LEFT JOIN (SELECT asset_id, latest_price FROM poly_dearboard.asset_latest_price FINAL) AS lp ON p.asset_id = lp.asset_id
            LEFT JOIN resolved rp ON p.asset_id = rp.asset_id
            WHERE lower(p.trader) = ?
            GROUP BY p.trader",
        )
        .bind(&address)
        .fetch_optional::<TraderSummary>()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    match result {
        Some(stats) => Ok(Json(stats)),
        None => Err((StatusCode::NOT_FOUND, "Trader not found".into())),
    }
}

pub async fn trader_trades(
    State(state): State<AppState>,
    Path(address): Path<String>,
    Query(params): Query<TradesParams>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let address = address.to_lowercase();
    let limit = params.limit.unwrap_or(50).min(200);
    let offset = params.offset.unwrap_or(0);
    let side_filter = params.side.as_deref().unwrap_or("");

    if !side_filter.is_empty() && side_filter != "buy" && side_filter != "sell" {
        return Err((
            StatusCode::BAD_REQUEST,
            "Invalid side filter. Allowed: buy, sell".into(),
        ));
    }

    let mut trades = state
        .db
        .query(
            "SELECT
                toString(tx_hash) AS tx_hash,
                block_number,
                ifNull(toString(block_timestamp), '') AS block_timestamp,
                exchange,
                side,
                asset_id,
                toString(amount) AS amount,
                toString(price) AS price,
                toString(usdc_amount) AS usdc_amount,
                toString(fee) AS fee
            FROM poly_dearboard.trades
            WHERE lower(trader) = ?
              AND (side = ? OR ? = '')
            ORDER BY block_number DESC, log_index DESC
            LIMIT ? OFFSET ?",
        )
        .bind(&address)
        .bind(side_filter)
        .bind(side_filter)
        .bind(limit)
        .bind(offset)
        .fetch_all::<TradeRecord>()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Replace ClickHouse asset_ids with full-precision Gamma token IDs (or integer fallback)
    {
        let token_ids: Vec<String> = trades.iter().map(|t| t.asset_id.clone()).collect();
        let market_info =
            markets::resolve_markets(&state.http, &state.db, &state.market_cache, &token_ids).await;
        for trade in &mut trades {
            trade.asset_id = market_info
                .get(&trade.asset_id)
                .map(|i| i.gamma_token_id.clone())
                .unwrap_or_else(|| markets::to_integer_id(&trade.asset_id));
        }
    }

    let total: u64 = state
        .db
        .query(
            "SELECT count() FROM poly_dearboard.trades WHERE lower(trader) = ? AND (side = ? OR ? = '')",
        )
        .bind(&address)
        .bind(side_filter)
        .bind(side_filter)
        .fetch_one()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(TradesResponse {
        trades,
        total,
        limit,
        offset,
    }))
}

pub async fn hot_markets(
    State(state): State<AppState>,
    Query(params): Query<HotMarketsParams>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let limit = params.limit.unwrap_or(20).min(100);
    let period = params.period.as_deref().unwrap_or("24h");

    // Fetch extra rows since Yes/No tokens will be merged into one event
    let fetch_limit = limit * 3;

    let rows = if period == "7d" {
        // Beyond 3-day TTL: read from pre-aggregated asset_stats_daily
        state
            .db
            .query(
                "SELECT
                    asset_id,
                    toString(sum(volume)) AS volume,
                    sum(trade_count) AS trade_count,
                    uniqExactMerge(unique_traders) AS unique_traders,
                    toString(argMaxMerge(last_price_state)) AS last_price,
                    ifNull(toString(max(last_trade)), '') AS last_trade
                FROM poly_dearboard.asset_stats_daily AS asd
                WHERE day >= today() - 7
                GROUP BY asset_id
                ORDER BY sum(asd.volume) DESC
                LIMIT ?",
            )
            .bind(fetch_limit)
            .fetch_all::<MarketStatsRow>()
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    } else {
        // Within 3-day TTL: read from raw trades
        let interval = match period {
            "1h" => "1 HOUR",
            _ => "24 HOUR",
        };
        let exclude = exclude_clause();

        let query = format!(
            "SELECT
                asset_id,
                toString(sum(usdc_amount)) AS volume,
                count() AS trade_count,
                uniqExact(trader) AS unique_traders,
                toString(argMax(price, block_number * 1000000 + log_index)) AS last_price,
                ifNull(toString(max(block_timestamp)), '') AS last_trade
            FROM poly_dearboard.trades
            PREWHERE block_timestamp >= now() - INTERVAL {interval}
            WHERE trader NOT IN ({exclude})
            GROUP BY asset_id
            ORDER BY sum(usdc_amount) DESC
            LIMIT ?"
        );

        state
            .db
            .query(&query)
            .bind(fetch_limit)
            .fetch_all::<MarketStatsRow>()
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    };

    let token_ids: Vec<String> = rows.iter().map(|r| r.asset_id.clone()).collect();
    let market_info =
        markets::resolve_markets(&state.http, &state.db, &state.market_cache, &token_ids).await;

    // Merge tokens belonging to the same event (Yes/No → one row)
    let mut merged: std::collections::HashMap<String, HotMarket> = std::collections::HashMap::new();

    for r in rows {
        let info = market_info.get(&r.asset_id);
        let question = info
            .map(|i| i.question.clone())
            .unwrap_or_else(|| shorten_id(&r.asset_id));
        // Prefer full-precision Gamma token ID; fall back to integer form (never scientific notation)
        let display_id = info
            .map(|i| i.gamma_token_id.clone())
            .unwrap_or_else(|| markets::to_integer_id(&r.asset_id));
        let vol: f64 = r.volume.parse().unwrap_or(0.0);

        if let Some(existing) = merged.get_mut(&question) {
            // Merge into existing event
            let existing_vol: f64 = existing.volume.parse().unwrap_or(0.0);
            existing.volume = format!("{:.6}", existing_vol + vol);
            existing.trade_count += r.trade_count;
            existing.unique_traders += r.unique_traders;
            existing.all_token_ids.push(display_id.clone());
            if r.last_trade > existing.last_trade {
                existing.last_trade = r.last_trade;
                existing.last_price = r.last_price;
            }
            // Keep the higher-volume token as the representative token_id
            if vol > existing_vol {
                existing.token_id = display_id;
            }
        } else {
            merged.insert(
                question.clone(),
                HotMarket {
                    token_id: display_id.clone(),
                    all_token_ids: vec![display_id],
                    question,
                    outcome: String::new(),
                    category: info.map(|i| i.category.clone()).unwrap_or_default(),
                    volume: r.volume,
                    trade_count: r.trade_count,
                    unique_traders: r.unique_traders,
                    last_price: r.last_price,
                    last_trade: r.last_trade,
                },
            );
        }
    }

    let mut markets: Vec<HotMarket> = merged.into_values().collect();
    markets.sort_by(|a, b| {
        let va: f64 = a.volume.parse().unwrap_or(0.0);
        let vb: f64 = b.volume.parse().unwrap_or(0.0);
        vb.partial_cmp(&va).unwrap_or(std::cmp::Ordering::Equal)
    });
    markets.truncate(limit as usize);

    Ok(Json(HotMarketsResponse { markets }))
}

pub async fn recent_trades(
    State(state): State<AppState>,
    Query(params): Query<LiveFeedParams>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let limit = params.limit.unwrap_or(50).min(200);
    let exclude = exclude_clause();

    // Support comma-separated token IDs for multi-outcome markets (Yes + No)
    let token_ids: Vec<String> = params
        .token_id
        .as_deref()
        .map(|s| {
            s.split(',')
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect()
        })
        .unwrap_or_default();

    // Validate token IDs to prevent SQL injection (must be numeric, possibly scientific notation)
    for id in &token_ids {
        if !id
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-'))
        {
            return Err((
                StatusCode::BAD_REQUEST,
                "Invalid token_id format".to_string(),
            ));
        }
    }

    // After UInt256 migration, asset_ids are full-precision integer strings.
    // Pass through as-is for exact matching.
    let token_ids: Vec<String> = token_ids.into_iter().map(String::from).collect();

    let query = if token_ids.is_empty() {
        format!(
            "SELECT
                toString(tx_hash) AS tx_hash,
                ifNull(toString(block_timestamp), '') AS block_timestamp,
                toString(trader) AS trader,
                side,
                asset_id,
                toString(amount) AS amount,
                toString(price) AS price,
                toString(usdc_amount) AS usdc_amount
            FROM poly_dearboard.trades
            WHERE trader NOT IN ({exclude})
            ORDER BY block_number DESC, log_index DESC
            LIMIT ?"
        )
    } else {
        let in_list = token_ids
            .iter()
            .map(|id| format!("'{}'", id.replace('\'', "''")))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "SELECT
                toString(tx_hash) AS tx_hash,
                ifNull(toString(block_timestamp), '') AS block_timestamp,
                toString(trader) AS trader,
                side,
                asset_id,
                toString(amount) AS amount,
                toString(price) AS price,
                toString(usdc_amount) AS usdc_amount
            FROM poly_dearboard.trades
            WHERE trader NOT IN ({exclude})
              AND asset_id IN ({in_list})
            ORDER BY block_number DESC, log_index DESC
            LIMIT ?"
        )
    };

    let rows = state
        .db
        .query(&query)
        .bind(limit)
        .fetch_all::<RecentTradeRow>()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let token_ids: Vec<String> = rows
        .iter()
        .map(|r| r.asset_id.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    let market_info =
        markets::resolve_markets(&state.http, &state.db, &state.market_cache, &token_ids).await;

    let trades = rows
        .into_iter()
        .map(|r| {
            let info = market_info.get(&r.asset_id);
            FeedTrade {
                question: info
                    .map(|i| i.question.clone())
                    .unwrap_or_else(|| shorten_id(&r.asset_id)),
                outcome: info.map(|i| i.outcome.clone()).unwrap_or_default(),
                category: info.map(|i| i.category.clone()).unwrap_or_default(),
                tx_hash: r.tx_hash,
                block_timestamp: r.block_timestamp,
                trader: r.trader,
                side: r.side,
                asset_id: info
                    .map(|i| i.gamma_token_id.clone())
                    .unwrap_or_else(|| markets::to_integer_id(&r.asset_id)),
                amount: r.amount,
                price: r.price,
                usdc_amount: r.usdc_amount,
            }
        })
        .collect();

    Ok(Json(LiveFeedResponse { trades }))
}

pub async fn health(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let stats = state
        .db
        .query(
            "SELECT
                sum(trade_count) AS trade_count,
                uniqExactMerge(unique_traders) AS trader_count,
                max(latest_block) AS latest_block
            FROM poly_dearboard.global_stats",
        )
        .fetch_one::<HealthStats>()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(HealthResponse {
        status: "ok",
        trade_count: stats.trade_count,
        trader_count: stats.trader_count,
        latest_block: stats.latest_block,
    }))
}

pub async fn trader_positions(
    State(state): State<AppState>,
    Path(address): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let address = address.to_lowercase();

    let rows = state
        .db
        .query(
            "WITH resolved AS (
                SELECT asset_id, toNullable(toFloat64(resolved_price)) AS resolved_price
                FROM poly_dearboard.resolved_prices FINAL
            )
            SELECT
                p.asset_id,
                if(p.buy_amount > p.sell_amount, 'long',
                   if(p.sell_amount > p.buy_amount, 'short', 'closed')) AS side_summary,
                toString(p.buy_amount - p.sell_amount) AS net_tokens,
                toString(if(p.buy_amount > toDecimal128(0, 6),
                    p.buy_usdc / p.buy_amount,
                    toDecimal128(0, 6))) AS cost_basis,
                toString(coalesce(rp.resolved_price, toFloat64(lp.latest_price))) AS latest_price,
                toString(ROUND((p.sell_usdc - p.buy_usdc) + (p.buy_amount - p.sell_amount) * coalesce(rp.resolved_price, toFloat64(lp.latest_price)), 6)) AS pnl,
                toString(p.total_volume) AS volume,
                p.trade_count AS trade_count,
                if(rp.resolved_price IS NOT NULL, 1, 0) AS on_chain_resolved
            FROM poly_dearboard.trader_positions p
            LEFT JOIN (SELECT asset_id, latest_price FROM poly_dearboard.asset_latest_price FINAL) AS lp ON p.asset_id = lp.asset_id
            LEFT JOIN resolved rp ON p.asset_id = rp.asset_id
            WHERE lower(p.trader) = ?
            ORDER BY abs((p.buy_amount - p.sell_amount) * coalesce(rp.resolved_price, toFloat64(lp.latest_price))) DESC",
        )
        .bind(&address)
        .fetch_all::<PositionRow>()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let token_ids: Vec<String> = rows.iter().map(|r| r.asset_id.clone()).collect();
    let market_info =
        markets::resolve_markets(&state.http, &state.db, &state.market_cache, &token_ids).await;

    let mut open = Vec::new();
    let mut closed = Vec::new();

    for r in rows {
        // On-chain resolution is the strongest signal (from condition_resolution table)
        let on_chain_resolved = r.on_chain_resolved == 1;
        let api_resolved = market_info
            .get(&r.asset_id)
            .map(|i| !i.active)
            .unwrap_or(false);
        // Price-based fallback: within half a cent of 0 or 1 means settled
        let price_settled = r
            .latest_price
            .parse::<f64>()
            .map(|p| p < 0.005 || p > 0.995)
            .unwrap_or(false);
        // Trader fully exited (bought then sold everything)
        let user_exited = r.side_summary == "closed";
        let settled = on_chain_resolved || api_resolved || price_settled || user_exited;

        let info = market_info.get(&r.asset_id);
        let pos = OpenPosition {
            question: info
                .map(|i| i.question.clone())
                .unwrap_or_else(|| shorten_id(&r.asset_id)),
            outcome: info.map(|i| i.outcome.clone()).unwrap_or_default(),
            asset_id: info
                .map(|i| i.gamma_token_id.clone())
                .unwrap_or_else(|| markets::to_integer_id(&r.asset_id)),
            side: r.side_summary,
            net_tokens: r.net_tokens,
            cost_basis: r.cost_basis,
            latest_price: r.latest_price,
            pnl: r.pnl,
            volume: r.volume,
            trade_count: r.trade_count,
        };

        if settled {
            closed.push(pos);
        } else {
            open.push(pos);
        }
    }

    Ok(Json(PositionsResponse { open, closed }))
}

pub async fn pnl_chart(
    State(state): State<AppState>,
    Path(address): Path<String>,
    Query(params): Query<PnlChartParams>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let address = address.to_lowercase();
    let timeframe = params.timeframe.as_deref().unwrap_or("all");

    // For windowed views: compute initial portfolio state before the window
    let mut asset_state: std::collections::HashMap<String, (f64, f64, f64)> =
        std::collections::HashMap::new();

    // 24h timeframe: raw trades (within TTL), hourly granularity
    // 7d/30d/all: pnl_daily aggregate table, daily granularity
    let use_aggregate = timeframe != "24h";

    if use_aggregate {
        // Read from pnl_daily for 7d/30d/all
        let day_filter = match timeframe {
            "7d" => Some(7),
            "30d" => Some(30),
            _ => None, // "all"
        };

        // Initial state: all pnl_daily rows BEFORE the window
        if let Some(days) = day_filter {
            let initial = state
                .db
                .query(&format!(
                    "SELECT
                        asset_id,
                        toString(sum(buy_amount) - sum(sell_amount)) AS net_tokens,
                        toString(sum(sell_usdc) - sum(buy_usdc)) AS cash_flow,
                        toString(argMaxMerge(last_price_state)) AS last_price
                    FROM poly_dearboard.pnl_daily
                    WHERE lower(trader) = ?
                      AND day < today() - {days}
                    GROUP BY asset_id"
                ))
                .bind(&address)
                .fetch_all::<PnlInitialStateRow>()
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

            for row in initial {
                let tokens = row.net_tokens.parse::<f64>().unwrap_or(0.0);
                let cash = row.cash_flow.parse::<f64>().unwrap_or(0.0);
                let price = row.last_price.parse::<f64>().unwrap_or(0.0);
                asset_state.insert(row.asset_id, (tokens, cash, price));
            }
        }

        // Window deltas from pnl_daily
        let day_where = day_filter
            .map(|d| format!("AND day >= today() - {d}"))
            .unwrap_or_default();

        let rows = state
            .db
            .query(&format!(
                "SELECT
                    toString(day) AS date,
                    asset_id,
                    toString(sum(buy_amount) - sum(sell_amount)) AS net_token_delta,
                    toString(sum(sell_usdc) - sum(buy_usdc)) AS cash_flow_delta,
                    toString(argMaxMerge(last_price_state)) AS last_price
                FROM poly_dearboard.pnl_daily
                WHERE lower(trader) = ?
                  {day_where}
                GROUP BY day, asset_id
                ORDER BY day, asset_id"
            ))
            .bind(&address)
            .fetch_all::<PnlDailyRow>()
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        if rows.is_empty() && asset_state.is_empty() {
            return Ok(Json(PnlChartResponse { points: vec![] }));
        }

        let resolved = fetch_resolved_prices(&state).await;
        let points = compute_pnl_points(rows, &mut asset_state, &resolved);
        return Ok(Json(PnlChartResponse { points }));
    }

    // 24h timeframe: read from raw trades with hourly granularity
    let initial = state
        .db
        .query(
            "SELECT
                asset_id,
                toString(sumIf(toFloat64(amount), side='buy') - sumIf(toFloat64(amount), side='sell')) AS net_tokens,
                toString(sumIf(toFloat64(usdc_amount), side='sell') - sumIf(toFloat64(usdc_amount), side='buy')) AS cash_flow,
                toString(argMax(toFloat64(price), block_number * 1000000 + log_index)) AS last_price
            FROM poly_dearboard.trades
            PREWHERE block_timestamp > toDateTime('1970-01-01 00:00:00')
              AND block_timestamp < now() - INTERVAL 24 HOUR
            WHERE lower(trader) = ?
            GROUP BY asset_id"
        )
        .bind(&address)
        .fetch_all::<PnlInitialStateRow>()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    for row in initial {
        let tokens = row.net_tokens.parse::<f64>().unwrap_or(0.0);
        let cash = row.cash_flow.parse::<f64>().unwrap_or(0.0);
        let price = row.last_price.parse::<f64>().unwrap_or(0.0);
        asset_state.insert(row.asset_id, (tokens, cash, price));
    }

    let rows = state
        .db
        .query(
            "SELECT
                ifNull(toString(toStartOfHour(block_timestamp)), '') AS date,
                asset_id,
                toString(sumIf(toFloat64(amount), side = 'buy') - sumIf(toFloat64(amount), side = 'sell')) AS net_token_delta,
                toString(sumIf(toFloat64(usdc_amount), side = 'sell') - sumIf(toFloat64(usdc_amount), side = 'buy')) AS cash_flow_delta,
                toString(argMax(toFloat64(price), block_number * 1000000 + log_index)) AS last_price
            FROM poly_dearboard.trades
            PREWHERE block_timestamp >= now() - INTERVAL 24 HOUR
            WHERE lower(trader) = ?
              AND block_timestamp > toDateTime('1970-01-01 00:00:00')
            GROUP BY toStartOfHour(block_timestamp), asset_id
            ORDER BY toStartOfHour(block_timestamp), asset_id"
        )
        .bind(&address)
        .fetch_all::<PnlDailyRow>()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if rows.is_empty() && asset_state.is_empty() {
        return Ok(Json(PnlChartResponse { points: vec![] }));
    }

    let resolved = fetch_resolved_prices(&state).await;
    let points = compute_pnl_points(rows, &mut asset_state, &resolved);
    Ok(Json(PnlChartResponse { points }))
}

/// Fetch resolved_prices lookup for PnL final-point overlay
async fn fetch_resolved_prices(state: &AppState) -> std::collections::HashMap<String, f64> {
    state
        .db
        .query("SELECT asset_id, resolved_price FROM poly_dearboard.resolved_prices FINAL")
        .fetch_all::<ResolvedPriceLookup>()
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|r| {
            r.resolved_price
                .parse::<f64>()
                .ok()
                .map(|p| (r.asset_id, p))
        })
        .collect()
}

/// Process bucket-by-bucket rows into PnL chart points
fn compute_pnl_points(
    rows: Vec<PnlDailyRow>,
    asset_state: &mut std::collections::HashMap<String, (f64, f64, f64)>,
    resolved: &std::collections::HashMap<String, f64>,
) -> Vec<PnlChartPoint> {
    let mut points: Vec<PnlChartPoint> = Vec::new();
    let mut current_date = String::new();

    for row in &rows {
        if !current_date.is_empty() && row.date != current_date {
            let pnl: f64 = asset_state
                .values()
                .map(|(tokens, cash, price)| cash + tokens * price)
                .sum();
            points.push(PnlChartPoint {
                date: current_date.clone(),
                pnl: format!("{:.2}", pnl),
            });
        }
        current_date.clone_from(&row.date);

        let delta_tokens = row.net_token_delta.parse::<f64>().unwrap_or(0.0);
        let delta_cash = row.cash_flow_delta.parse::<f64>().unwrap_or(0.0);
        let price = row.last_price.parse::<f64>().unwrap_or(0.0);

        let entry = asset_state
            .entry(row.asset_id.clone())
            .or_insert((0.0, 0.0, 0.0));
        entry.0 += delta_tokens;
        entry.1 += delta_cash;
        entry.2 = price;
    }

    // Final point: use resolved prices where available (COALESCE equivalent)
    if !current_date.is_empty() {
        let pnl: f64 = asset_state
            .iter()
            .map(|(asset_id, (tokens, cash, price))| {
                let final_price = resolved.get(asset_id).copied().unwrap_or(*price);
                cash + tokens * final_price
            })
            .sum();
        points.push(PnlChartPoint {
            date: current_date,
            pnl: format!("{:.2}", pnl),
        });
    }

    points
}

pub async fn resolve_market(
    State(state): State<AppState>,
    Query(params): Query<ResolveParams>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let token_ids: Vec<String> = params
        .token_ids
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if token_ids.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "token_ids required".to_string()));
    }

    let info =
        markets::resolve_markets(&state.http, &state.db, &state.market_cache, &token_ids).await;

    let mut resolved: std::collections::HashMap<String, ResolvedMarket> =
        std::collections::HashMap::new();
    for (id, m) in info {
        let market = ResolvedMarket {
            question: m.question,
            outcome: m.outcome,
            category: m.category,
            active: m.active,
            gamma_token_id: m.gamma_token_id.clone(),
            all_token_ids: m.all_token_ids,
            outcomes: m.outcomes,
        };
        // Key by both input ID and gamma_token_id so frontend lookups
        // work regardless of which asset_id format is used.
        if id != m.gamma_token_id {
            resolved.insert(id, market.clone());
        }
        resolved.insert(m.gamma_token_id, market);
    }

    Ok(Json(resolved))
}

// -- Wallet Auth (EIP-712 + JWT) --

#[derive(Deserialize)]
pub struct NonceParams {
    pub address: String,
}

#[derive(Deserialize)]
pub struct VerifyBody {
    pub address: String,
    pub signature: String,
    pub nonce: String,
    pub issued_at: String,
}

pub async fn auth_nonce(
    State(state): State<AppState>,
    Query(params): Query<NonceParams>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let user_db = state.user_db.clone();
    let address = params.address.to_lowercase();

    let (nonce, issued_at) = tokio::task::spawn_blocking(move || {
        let conn = user_db.lock().expect("user_db lock poisoned");
        super::db::get_or_create_user(&conn, &address)
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(
        serde_json::json!({ "nonce": nonce, "issuedAt": issued_at }),
    ))
}

pub async fn auth_verify(
    State(state): State<AppState>,
    Json(body): Json<VerifyBody>,
) -> Result<impl IntoResponse, super::auth::AuthError> {
    let address = body.address.to_lowercase();
    let signature = body.signature.clone();
    let nonce = body.nonce.clone();
    let issued_at = body.issued_at.clone();
    let jwt_secret = state.jwt_secret.clone();

    // Atomic: verify signature + check nonce + rotate — all under the lock
    let user_db = state.user_db.clone();
    let token = tokio::task::spawn_blocking(move || -> Result<String, super::auth::AuthError> {
        // Verify EIP-712 signature
        super::auth::recover_eip712_signer(&address, &nonce, &issued_at, &signature)?;

        // Verify nonce + issued_at match DB, then rotate
        let conn = user_db.lock().expect("user_db lock poisoned");
        let valid = super::db::verify_and_rotate_nonce(&conn, &address, &nonce, &issued_at)
            .map_err(|_| super::auth::AuthError::InvalidToken)?;

        if !valid {
            return Err(super::auth::AuthError::NonceMismatch);
        }

        Ok(super::auth::issue_jwt(&address, &jwt_secret))
    })
    .await
    .map_err(|_| super::auth::AuthError::InvalidToken)??;

    let address = body.address.to_lowercase();
    Ok(Json(
        serde_json::json!({ "token": token, "address": address }),
    ))
}

pub async fn smart_money(
    State(state): State<AppState>,
    Query(params): Query<SmartMoneyParams>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let exclude = exclude_clause();
    let top = params.top.unwrap_or(10).clamp(1, 50);
    let timeframe = params.timeframe.as_deref().unwrap_or("all");

    let rows = if timeframe == "all" {
        // All-time: read from pre-aggregated trader_positions
        let query = format!(
            "WITH
                resolved AS (
                    SELECT asset_id, toNullable(toFloat64(resolved_price)) AS resolved_price
                    FROM poly_dearboard.resolved_prices FINAL
                ),
                trader_pnl AS (
                    SELECT p.trader,
                           sum((p.sell_usdc - p.buy_usdc) + (p.buy_amount - p.sell_amount) * coalesce(rp.resolved_price, toFloat64(lp.latest_price))) AS total_pnl
                    FROM poly_dearboard.trader_positions p
                    LEFT JOIN (SELECT asset_id, latest_price FROM poly_dearboard.asset_latest_price FINAL) AS lp ON p.asset_id = lp.asset_id
                    LEFT JOIN resolved rp ON p.asset_id = rp.asset_id
                    WHERE p.trader NOT IN ({exclude})
                    GROUP BY p.trader
                    ORDER BY total_pnl DESC
                    LIMIT {top}
                ),
                smart_positions AS (
                    SELECT p.asset_id AS asset_id,
                           (p.buy_amount - p.sell_amount) AS net_tokens,
                           toFloat64(lp.latest_price) AS price,
                           toFloat64(p.buy_amount - p.sell_amount) * toFloat64(lp.latest_price) AS exposure
                    FROM poly_dearboard.trader_positions p
                    LEFT JOIN (SELECT asset_id, latest_price FROM poly_dearboard.asset_latest_price FINAL) AS lp ON p.asset_id = lp.asset_id
                    LEFT JOIN resolved rp ON p.asset_id = rp.asset_id
                    WHERE p.trader IN (SELECT trader FROM trader_pnl)
                      AND rp.resolved_price IS NULL
                      AND toFloat64(lp.latest_price) > 0.01
                      AND toFloat64(lp.latest_price) < 0.99
                      AND abs(p.buy_amount - p.sell_amount) > 0.01
                )
            SELECT
                asset_id,
                count() AS smart_trader_count,
                countIf(net_tokens > 0) AS long_count,
                countIf(net_tokens < 0) AS short_count,
                toString(sum(if(net_tokens > 0, exposure, toFloat64(0)))) AS long_exposure,
                toString(sum(if(net_tokens < 0, abs(exposure), toFloat64(0)))) AS short_exposure,
                toString(avg(price)) AS avg_price
            FROM smart_positions
            GROUP BY asset_id
            ORDER BY count() DESC, sum(abs(exposure)) DESC
            LIMIT 20"
        );

        state
            .db
            .query(&query)
            .fetch_all::<SmartMoneyRow>()
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    } else {
        // Time-windowed (1h/24h): read from raw trades (within TTL) + asset_latest_price
        let prewhere = match timeframe {
            "1h" => "PREWHERE block_timestamp >= now() - INTERVAL 1 HOUR",
            "24h" => "PREWHERE block_timestamp >= now() - INTERVAL 24 HOUR",
            _ => "",
        };

        let query = format!(
            "WITH
                resolved AS (
                    SELECT asset_id, toNullable(toFloat64(resolved_price)) AS resolved_price
                    FROM poly_dearboard.resolved_prices FINAL
                ),
                trader_pnl AS (
                    SELECT trader,
                           sum(cash_flow + net_tokens * coalesce(rp.resolved_price, toFloat64(lp.latest_price))) AS total_pnl
                    FROM (
                        SELECT trader, asset_id,
                               sumIf(amount, side = 'buy') - sumIf(amount, side = 'sell') AS net_tokens,
                               sumIf(usdc_amount, side = 'sell') - sumIf(usdc_amount, side = 'buy') AS cash_flow
                        FROM poly_dearboard.trades
                        {prewhere}
                        WHERE trader NOT IN ({exclude})
                        GROUP BY trader, asset_id
                    ) p
                    LEFT JOIN (SELECT asset_id, latest_price FROM poly_dearboard.asset_latest_price FINAL) AS lp ON p.asset_id = lp.asset_id
                    LEFT JOIN resolved rp ON p.asset_id = rp.asset_id
                    GROUP BY trader
                    ORDER BY total_pnl DESC
                    LIMIT {top}
                ),
                smart_positions AS (
                    SELECT p.asset_id AS asset_id,
                           p.net_tokens AS net_tokens,
                           toFloat64(lp.latest_price) AS price,
                           p.net_tokens * toFloat64(lp.latest_price) AS exposure
                    FROM (
                        SELECT trader, asset_id,
                               sumIf(amount, side = 'buy') - sumIf(amount, side = 'sell') AS net_tokens
                        FROM poly_dearboard.trades
                        WHERE trader IN (SELECT trader FROM trader_pnl)
                        GROUP BY trader, asset_id
                        HAVING abs(net_tokens) > 0.01
                    ) p
                    LEFT JOIN (SELECT asset_id, latest_price FROM poly_dearboard.asset_latest_price FINAL) AS lp ON p.asset_id = lp.asset_id
                    LEFT JOIN resolved rp ON p.asset_id = rp.asset_id
                    WHERE rp.resolved_price IS NULL
                      AND toFloat64(lp.latest_price) > 0.01
                      AND toFloat64(lp.latest_price) < 0.99
                )
            SELECT
                asset_id,
                count() AS smart_trader_count,
                countIf(net_tokens > 0) AS long_count,
                countIf(net_tokens < 0) AS short_count,
                toString(sum(if(net_tokens > 0, exposure, 0))) AS long_exposure,
                toString(sum(if(net_tokens < 0, abs(exposure), 0))) AS short_exposure,
                toString(avg(price)) AS avg_price
            FROM smart_positions
            GROUP BY asset_id
            ORDER BY count() DESC, sum(abs(exposure)) DESC
            LIMIT 20"
        );

        state
            .db
            .query(&query)
            .fetch_all::<SmartMoneyRow>()
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    };

    let token_ids: Vec<String> = rows.iter().map(|r| r.asset_id.clone()).collect();
    let market_info =
        markets::resolve_markets(&state.http, &state.db, &state.market_cache, &token_ids).await;

    // Merge Yes/No tokens of the same market into one entry
    let mut merged: std::collections::HashMap<String, SmartMoneyMarket> =
        std::collections::HashMap::new();

    for r in rows {
        let info = market_info.get(&r.asset_id);

        // Skip resolved/inactive/uncached markets
        if info.map(|i| !i.active).unwrap_or(true) {
            continue;
        }

        let info = info.unwrap(); // safe: None handled above
        let question = info.question.clone();
        let token_id = info.gamma_token_id.clone();
        let outcome = info.outcome.clone();

        let long_exp: f64 = r.long_exposure.parse().unwrap_or(0.0);
        let short_exp: f64 = r.short_exposure.parse().unwrap_or(0.0);

        if let Some(existing) = merged.get_mut(&question) {
            let existing_long: f64 = existing.long_exposure.parse().unwrap_or(0.0);
            let existing_short: f64 = existing.short_exposure.parse().unwrap_or(0.0);
            existing.long_exposure = format!("{:.6}", existing_long + long_exp);
            existing.short_exposure = format!("{:.6}", existing_short + short_exp);
            existing.long_count += r.long_count;
            existing.short_count += r.short_count;
            existing.smart_trader_count = existing.smart_trader_count.max(r.smart_trader_count);
        } else {
            merged.insert(
                question.clone(),
                SmartMoneyMarket {
                    token_id,
                    question,
                    outcome,
                    smart_trader_count: r.smart_trader_count,
                    long_count: r.long_count,
                    short_count: r.short_count,
                    long_exposure: r.long_exposure,
                    short_exposure: r.short_exposure,
                    avg_price: r.avg_price,
                },
            );
        }
    }

    let mut markets: Vec<SmartMoneyMarket> = merged.into_values().collect();
    markets.sort_by(|a, b| {
        b.smart_trader_count
            .cmp(&a.smart_trader_count)
            .then_with(|| {
                let a_total: f64 = a.long_exposure.parse::<f64>().unwrap_or(0.0)
                    + a.short_exposure.parse::<f64>().unwrap_or(0.0);
                let b_total: f64 = b.long_exposure.parse::<f64>().unwrap_or(0.0)
                    + b.short_exposure.parse::<f64>().unwrap_or(0.0);
                b_total
                    .partial_cmp(&a_total)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    markets.truncate(10);

    Ok(Json(SmartMoneyResponse { markets, top }))
}

pub async fn trader_profile(
    State(state): State<AppState>,
    Path(address): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let address = address.to_lowercase();

    // Query 1: aggregate stats
    let agg = state
        .db
        .query(
            "WITH resolved AS (
                SELECT asset_id, resolved_price FROM poly_dearboard.resolved_prices FINAL
            )
            SELECT
                toString(ROUND(avg(tp.total_volume), 6)) AS avg_position_size,
                avg(if(tp.first_ts IS NOT NULL AND tp.last_ts IS NOT NULL,
                    dateDiff('hour', tp.first_ts, tp.last_ts), 0)) AS avg_hold_time_hours,
                count() AS total_positions,
                countIf(rp.asset_id != '') AS resolved_positions
            FROM poly_dearboard.trader_positions tp FINAL
            LEFT JOIN resolved rp ON tp.asset_id = rp.asset_id
            WHERE lower(tp.trader) = ?",
        )
        .bind(&address)
        .fetch_optional::<ProfileAggRow>()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let agg = match agg {
        Some(a) => a,
        None => return Err((StatusCode::NOT_FOUND, "Trader not found".into())),
    };

    // Query 2: all positions with PnL (for biggest win/loss, categories, labels)
    let positions = state
        .db
        .query(
            "WITH resolved AS (
                SELECT asset_id, toNullable(toFloat64(resolved_price)) AS resolved_price
                FROM poly_dearboard.resolved_prices FINAL
            )
            SELECT
                tp.asset_id,
                toString(ROUND((tp.sell_usdc - tp.buy_usdc)
                    + (tp.buy_amount - tp.sell_amount)
                    * coalesce(rp.resolved_price, toFloat64(lp.latest_price)), 6)) AS pnl,
                toString(tp.total_volume) AS total_volume,
                tp.trade_count,
                toString(tp.buy_amount - tp.sell_amount) AS net_tokens,
                ifNull(toString(tp.first_ts), '') AS first_ts,
                ifNull(toString(tp.last_ts), '') AS last_ts,
                ifNull(toString(rp.resolved_price), '') AS resolved_price,
                if(rp.resolved_price IS NOT NULL, 1, 0) AS on_chain_resolved,
                toString(coalesce(toFloat64(lp.latest_price), 0)) AS latest_price,
                toString(tp.buy_usdc) AS buy_usdc,
                toString(tp.sell_usdc) AS sell_usdc,
                toString(tp.buy_amount) AS buy_amount
            FROM poly_dearboard.trader_positions tp FINAL
            LEFT JOIN (SELECT asset_id, latest_price FROM poly_dearboard.asset_latest_price FINAL) lp
                ON tp.asset_id = lp.asset_id
            LEFT JOIN resolved rp ON tp.asset_id = rp.asset_id
            WHERE lower(tp.trader) = ?",
        )
        .bind(&address)
        .fetch_all::<ProfilePositionRow>()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Resolve market metadata for all positions
    let token_ids: Vec<String> = positions.iter().map(|p| p.asset_id.clone()).collect();
    let market_info =
        markets::resolve_markets(&state.http, &state.db, &state.market_cache, &token_ids).await;

    // Biggest win / biggest loss
    let mut best_win: Option<(f64, &ProfilePositionRow)> = None;
    let mut best_loss: Option<(f64, &ProfilePositionRow)> = None;

    for p in &positions {
        let pnl: f64 = p.pnl.parse().unwrap_or(0.0);
        if pnl > 0.0 && best_win.map(|(v, _)| pnl > v).unwrap_or(true) {
            best_win = Some((pnl, p));
        }
        if pnl < 0.0 && best_loss.map(|(v, _)| pnl < v).unwrap_or(true) {
            best_loss = Some((pnl, p));
        }
    }

    let to_highlight = |row: &ProfilePositionRow| -> PositionHighlight {
        let info = market_info.get(&row.asset_id);
        PositionHighlight {
            asset_id: info
                .map(|i| i.gamma_token_id.clone())
                .unwrap_or_else(|| markets::to_integer_id(&row.asset_id)),
            question: info
                .map(|i| i.question.clone())
                .unwrap_or_else(|| shorten_id(&row.asset_id)),
            outcome: info.map(|i| i.outcome.clone()).unwrap_or_default(),
            pnl: row.pnl.clone(),
        }
    };

    let biggest_win = best_win.map(|(_, r)| to_highlight(r));
    let biggest_loss = best_loss.map(|(_, r)| to_highlight(r));

    // Category breakdown (hybrid SQL + Rust via MarketCache)
    let mut cat_map: std::collections::HashMap<String, (f64, u64, f64)> =
        std::collections::HashMap::new();
    let mut total_volume: f64 = 0.0;
    let mut total_trade_count: u64 = 0;
    let mut earliest_ts: Option<&str> = None;
    let mut latest_ts: Option<&str> = None;

    for p in &positions {
        let category = market_info
            .get(&p.asset_id)
            .map(|i| i.category.clone())
            .unwrap_or_else(|| "Unknown".to_string());
        let vol: f64 = p.total_volume.parse().unwrap_or(0.0);
        let pnl: f64 = p.pnl.parse().unwrap_or(0.0);
        let entry = cat_map.entry(category).or_insert((0.0, 0, 0.0));
        entry.0 += vol;
        entry.1 += p.trade_count;
        entry.2 += pnl;
        total_volume += vol;
        total_trade_count += p.trade_count;

        if !p.first_ts.is_empty() {
            if earliest_ts.map(|e| p.first_ts.as_str() < e).unwrap_or(true) {
                earliest_ts = Some(&p.first_ts);
            }
        }
        if !p.last_ts.is_empty() {
            if latest_ts.map(|l| p.last_ts.as_str() > l).unwrap_or(true) {
                latest_ts = Some(&p.last_ts);
            }
        }
    }

    let mut category_breakdown: Vec<CategoryStats> = cat_map
        .into_iter()
        .map(|(cat, (vol, tc, pnl))| CategoryStats {
            category: cat,
            volume: format!("{:.6}", vol),
            trade_count: tc,
            pnl: format!("{:.6}", pnl),
        })
        .collect();
    category_breakdown.sort_by(|a, b| {
        let va: f64 = a.volume.parse().unwrap_or(0.0);
        let vb: f64 = b.volume.parse().unwrap_or(0.0);
        vb.partial_cmp(&va).unwrap_or(std::cmp::Ordering::Equal)
    });

    // Compute active span
    let active_span_days = match (earliest_ts, latest_ts) {
        (Some(e), Some(l)) => {
            let early = chrono::NaiveDateTime::parse_from_str(e, "%Y-%m-%d %H:%M:%S")
                .or_else(|_| chrono::NaiveDateTime::parse_from_str(e, "%Y-%m-%dT%H:%M:%S"));
            let late = chrono::NaiveDateTime::parse_from_str(l, "%Y-%m-%d %H:%M:%S")
                .or_else(|_| chrono::NaiveDateTime::parse_from_str(l, "%Y-%m-%dT%H:%M:%S"));
            match (early, late) {
                (Ok(e), Ok(l)) => (l - e).num_hours() as f64 / 24.0,
                _ => 0.0,
            }
        }
        _ => 0.0,
    };

    let (labels, label_details) = compute_labels(
        &positions,
        &market_info,
        &category_breakdown,
        total_volume,
        total_trade_count,
        positions.len() as u64,
        active_span_days,
    );

    Ok(Json(TraderProfile {
        avg_position_size: agg.avg_position_size,
        avg_hold_time_hours: agg.avg_hold_time_hours,
        biggest_win,
        biggest_loss,
        category_breakdown,
        total_positions: agg.total_positions,
        resolved_positions: agg.resolved_positions,
        labels,
        label_details,
    }))
}

/// Batch-compute labels for a list of traders (used by leaderboard).
/// Returns empty map on error — leaderboard still works without labels.
async fn batch_compute_labels(
    state: &AppState,
    addresses: &[String],
) -> (
    std::collections::HashMap<String, Vec<BehavioralLabel>>,
    std::collections::HashMap<String, LabelDetails>,
) {
    let mut result = std::collections::HashMap::new();
    let mut details_map = std::collections::HashMap::new();
    if addresses.is_empty() {
        return (result, details_map);
    }

    let in_list = addresses
        .iter()
        .map(|a| format!("'{}'", a.replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(",");

    let t0 = std::time::Instant::now();
    let positions: Vec<BatchPositionRow> = match state
        .db
        .query(&format!(
            "WITH resolved AS (
                SELECT asset_id, toNullable(toFloat64(resolved_price)) AS resolved_price
                FROM poly_dearboard.resolved_prices FINAL
            ),
            filtered AS (
                SELECT trader, asset_id,
                       sum(buy_usdc) AS buy_usdc, sum(sell_usdc) AS sell_usdc,
                       sum(buy_amount) AS buy_amount, sum(sell_amount) AS sell_amount,
                       sum(total_volume) AS total_volume, sum(trade_count) AS trade_count,
                       min(first_ts) AS first_ts, max(last_ts) AS last_ts
                FROM poly_dearboard.trader_positions
                WHERE lower(trader) IN ({in_list})
                GROUP BY trader, asset_id
            )
            SELECT
                toString(tp.trader) AS trader,
                tp.asset_id,
                toString(ROUND((tp.sell_usdc - tp.buy_usdc)
                    + (tp.buy_amount - tp.sell_amount)
                    * coalesce(rp.resolved_price, toFloat64(lp.latest_price)), 6)) AS pnl,
                toString(tp.total_volume) AS total_volume,
                tp.trade_count,
                toString(tp.buy_amount - tp.sell_amount) AS net_tokens,
                ifNull(toString(tp.first_ts), '') AS first_ts,
                ifNull(toString(tp.last_ts), '') AS last_ts,
                ifNull(toString(rp.resolved_price), '') AS resolved_price,
                if(rp.resolved_price IS NOT NULL, 1, 0) AS on_chain_resolved,
                toString(coalesce(toFloat64(lp.latest_price), 0)) AS latest_price,
                toString(tp.buy_usdc) AS buy_usdc,
                toString(tp.sell_usdc) AS sell_usdc,
                toString(tp.buy_amount) AS buy_amount
            FROM filtered tp
            LEFT JOIN (SELECT asset_id, latest_price FROM poly_dearboard.asset_latest_price FINAL) lp
                ON tp.asset_id = lp.asset_id
            LEFT JOIN resolved rp ON tp.asset_id = rp.asset_id"
        ))
        .fetch_all()
        .await
    {
        Ok(rows) => {
            tracing::debug!("batch labels: CH query returned {} rows in {:?}", rows.len(), t0.elapsed());
            rows
        }
        Err(e) => {
            tracing::warn!("Failed to batch-query positions for labels: {e}");
            return (result, details_map);
        }
    };

    // Cache-only market info lookup (no Gamma API calls — leaderboard must stay fast)
    let market_info: std::collections::HashMap<String, markets::MarketInfo> = {
        let cache = state.market_cache.read().await;
        let mut info = std::collections::HashMap::new();
        for p in &positions {
            let key = markets::cache_key(&p.asset_id);
            if let Some(m) = cache.get(&key) {
                info.insert(p.asset_id.clone(), m.clone());
            }
        }
        info
    };

    // Group positions by trader
    let mut by_trader: std::collections::HashMap<String, Vec<ProfilePositionRow>> =
        std::collections::HashMap::new();
    for p in positions {
        by_trader
            .entry(p.trader.to_lowercase())
            .or_default()
            .push(ProfilePositionRow {
                asset_id: p.asset_id,
                pnl: p.pnl,
                total_volume: p.total_volume,
                trade_count: p.trade_count,
                net_tokens: p.net_tokens,
                first_ts: p.first_ts,
                last_ts: p.last_ts,
                resolved_price: p.resolved_price,
                on_chain_resolved: p.on_chain_resolved,
                latest_price: p.latest_price,
                buy_usdc: p.buy_usdc,
                sell_usdc: p.sell_usdc,
                buy_amount: p.buy_amount,
            });
    }

    // Compute labels per trader
    for (addr, positions) in &by_trader {
        let mut cat_map: std::collections::HashMap<String, (f64, u64, f64)> =
            std::collections::HashMap::new();
        let mut total_volume: f64 = 0.0;
        let mut total_trade_count: u64 = 0;
        let mut earliest_ts: Option<&str> = None;
        let mut latest_ts: Option<&str> = None;

        for p in positions {
            let category = market_info
                .get(&p.asset_id)
                .map(|i| i.category.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            let vol: f64 = p.total_volume.parse().unwrap_or(0.0);
            let pnl: f64 = p.pnl.parse().unwrap_or(0.0);
            let entry = cat_map.entry(category).or_insert((0.0, 0, 0.0));
            entry.0 += vol;
            entry.1 += p.trade_count;
            entry.2 += pnl;
            total_volume += vol;
            total_trade_count += p.trade_count;

            if !p.first_ts.is_empty() {
                if earliest_ts.map(|e| p.first_ts.as_str() < e).unwrap_or(true) {
                    earliest_ts = Some(&p.first_ts);
                }
            }
            if !p.last_ts.is_empty() {
                if latest_ts.map(|l| p.last_ts.as_str() > l).unwrap_or(true) {
                    latest_ts = Some(&p.last_ts);
                }
            }
        }

        let mut category_breakdown: Vec<CategoryStats> = cat_map
            .into_iter()
            .map(|(cat, (vol, tc, pnl))| CategoryStats {
                category: cat,
                volume: format!("{:.6}", vol),
                trade_count: tc,
                pnl: format!("{:.6}", pnl),
            })
            .collect();
        category_breakdown.sort_by(|a, b| {
            let va: f64 = a.volume.parse().unwrap_or(0.0);
            let vb: f64 = b.volume.parse().unwrap_or(0.0);
            vb.partial_cmp(&va).unwrap_or(std::cmp::Ordering::Equal)
        });

        let active_span_days = match (earliest_ts, latest_ts) {
            (Some(e), Some(l)) => {
                let early = chrono::NaiveDateTime::parse_from_str(e, "%Y-%m-%d %H:%M:%S")
                    .or_else(|_| chrono::NaiveDateTime::parse_from_str(e, "%Y-%m-%dT%H:%M:%S"));
                let late = chrono::NaiveDateTime::parse_from_str(l, "%Y-%m-%d %H:%M:%S")
                    .or_else(|_| chrono::NaiveDateTime::parse_from_str(l, "%Y-%m-%dT%H:%M:%S"));
                match (early, late) {
                    (Ok(e), Ok(l)) => (l - e).num_hours() as f64 / 24.0,
                    _ => 0.0,
                }
            }
            _ => 0.0,
        };

        let (labels, details) = compute_labels(
            positions,
            &market_info,
            &category_breakdown,
            total_volume,
            total_trade_count,
            positions.len() as u64,
            active_span_days,
        );

        if !labels.is_empty() {
            result.insert(addr.clone(), labels);
            details_map.insert(addr.clone(), details);
        }
    }

    (result, details_map)
}

fn compute_labels(
    positions: &[ProfilePositionRow],
    market_info: &std::collections::HashMap<String, markets::MarketInfo>,
    category_breakdown: &[CategoryStats],
    total_volume: f64,
    total_trade_count: u64,
    unique_markets: u64,
    active_span_days: f64,
) -> (Vec<BehavioralLabel>, LabelDetails) {
    let mut labels = Vec::new();

    // Win rate + z-score from settled positions
    // "Settled" = on-chain resolved OR price near 0/1 (de facto decided)
    let mut settled_count: u64 = 0;
    let mut correct_count: u64 = 0;

    for p in positions {
        let lp: f64 = p.latest_price.parse().unwrap_or(0.5);
        let is_settled = p.on_chain_resolved == 1 || lp >= 0.95 || lp <= 0.05;
        if !is_settled {
            continue;
        }

        let effective_price: f64 = if p.on_chain_resolved == 1 {
            p.resolved_price.parse().unwrap_or(0.5)
        } else if lp >= 0.95 {
            1.0
        } else {
            0.0
        };

        let net: f64 = p.net_tokens.parse().unwrap_or(0.0);
        if net.abs() < 1e-9 {
            continue; // fully closed before settlement, skip
        }
        settled_count += 1;
        let is_correct =
            (net > 0.0 && effective_price > 0.5) || (net < 0.0 && effective_price < 0.5);
        if is_correct {
            correct_count += 1;
        }
    }

    let win_rate = if settled_count > 0 {
        (correct_count as f64 / settled_count as f64) * 100.0
    } else {
        0.0
    };

    let z_score = if settled_count >= 2 {
        let n = settled_count as f64;
        (correct_count as f64 - n * 0.5) / (n * 0.25_f64).sqrt()
    } else {
        0.0
    };

    // Category dominance
    let (dominant_category, dominant_pct, cat_win_rate) =
        if !category_breakdown.is_empty() && total_volume > 0.0 {
            let top = &category_breakdown[0]; // already sorted by volume desc
            let top_vol: f64 = top.volume.parse().unwrap_or(0.0);
            let pct = (top_vol / total_volume) * 100.0;

            // Category win rate from settled positions in this category
            let mut cat_settled = 0u64;
            let mut cat_correct = 0u64;
            for p in positions {
                let lp: f64 = p.latest_price.parse().unwrap_or(0.5);
                let is_settled = p.on_chain_resolved == 1 || lp >= 0.95 || lp <= 0.05;
                if !is_settled {
                    continue;
                }
                let cat = market_info
                    .get(&p.asset_id)
                    .map(|i| i.category.as_str())
                    .unwrap_or("Unknown");
                if cat != top.category {
                    continue;
                }
                let effective_price: f64 = if p.on_chain_resolved == 1 {
                    p.resolved_price.parse().unwrap_or(0.5)
                } else if lp >= 0.95 {
                    1.0
                } else {
                    0.0
                };
                let net: f64 = p.net_tokens.parse().unwrap_or(0.0);
                if net.abs() < 1e-9 {
                    continue;
                }
                cat_settled += 1;
                if (net > 0.0 && effective_price > 0.5) || (net < 0.0 && effective_price < 0.5) {
                    cat_correct += 1;
                }
            }
            let cwr = if cat_settled > 0 {
                (cat_correct as f64 / cat_settled as f64) * 100.0
            } else {
                0.0
            };
            (top.category.clone(), pct, cwr)
        } else {
            (String::new(), 0.0, 0.0)
        };

    let avg_position = if unique_markets > 0 {
        total_volume / unique_markets as f64
    } else {
        0.0
    };

    // Buy/sell balance for Market Maker detection
    let total_buy: f64 = positions
        .iter()
        .map(|p| p.buy_usdc.parse::<f64>().unwrap_or(0.0))
        .sum();
    let total_sell: f64 = positions
        .iter()
        .map(|p| p.sell_usdc.parse::<f64>().unwrap_or(0.0))
        .sum();
    let buy_sell_ratio = if total_buy.max(total_sell) > 0.0 {
        total_buy.min(total_sell) / total_buy.max(total_sell)
    } else {
        0.0
    };

    let trades_per_market = if unique_markets > 0 {
        total_trade_count as f64 / unique_markets as f64
    } else {
        0.0
    };

    // --- Labels (not mutually exclusive) ---

    // Sharp: skilled trader, statistically significant edge
    if win_rate >= 60.0 && settled_count >= 10 && z_score > 1.5 {
        labels.push(BehavioralLabel::Sharp);
    }

    // Specialist: concentrated in one category
    let cat_settled_count: u64 = positions
        .iter()
        .filter(|p| {
            let lp: f64 = p.latest_price.parse().unwrap_or(0.5);
            let is_settled = p.on_chain_resolved == 1 || lp >= 0.95 || lp <= 0.05;
            is_settled
                && market_info
                    .get(&p.asset_id)
                    .map(|i| i.category == dominant_category)
                    .unwrap_or(false)
        })
        .count() as u64;
    let is_specialist = if cat_settled_count >= 5 {
        dominant_pct > 70.0 && cat_win_rate > 55.0
    } else {
        dominant_pct >= 80.0 && total_volume > 10_000.0 && total_trade_count >= 10
    };
    if is_specialist && !dominant_category.is_empty() && dominant_category != "Unknown" {
        labels.push(BehavioralLabel::Specialist);
    }

    // Whale: large concentrated bets
    if total_volume > 100_000.0 && avg_position > 5_000.0 && unique_markets < 30 {
        labels.push(BehavioralLabel::Whale);
    }

    // Degen: high volume, poor win rate — no edge
    if win_rate < 40.0 && settled_count >= 10 && total_volume > 5_000.0 {
        labels.push(BehavioralLabel::Degen);
    }

    // Market Maker: balanced buy/sell, high activity across many markets
    if buy_sell_ratio > 0.6 && total_trade_count >= 50 && unique_markets >= 10 {
        labels.push(BehavioralLabel::MarketMaker);
    }

    // Bot: high trade frequency per market (constant rebalancing)
    if total_trade_count >= 200 && trades_per_market >= 15.0 {
        labels.push(BehavioralLabel::Bot);
    }

    // Contrarian: buys cheap (unpopular) outcomes that settle correctly
    let mut contrarian_trades: u64 = 0;
    let mut contrarian_correct: u64 = 0;
    for p in positions {
        let net: f64 = p.net_tokens.parse().unwrap_or(0.0);
        if net <= 0.0 {
            continue;
        }
        let lp: f64 = p.latest_price.parse().unwrap_or(0.5);
        let is_settled = p.on_chain_resolved == 1 || lp >= 0.95 || lp <= 0.05;
        if !is_settled {
            continue;
        }
        let buy_amt: f64 = p.buy_amount.parse().unwrap_or(0.0);
        if buy_amt < 1e-9 {
            continue;
        }
        let buy_usd: f64 = p.buy_usdc.parse().unwrap_or(0.0);
        let avg_cost = buy_usd / buy_amt;
        if avg_cost < 0.30 {
            contrarian_trades += 1;
            let effective_price: f64 = if p.on_chain_resolved == 1 {
                p.resolved_price.parse().unwrap_or(0.5)
            } else if lp >= 0.95 {
                1.0
            } else {
                0.0
            };
            if effective_price > 0.5 {
                contrarian_correct += 1;
            }
        }
    }
    let contrarian_rate = if contrarian_trades > 0 {
        (contrarian_correct as f64 / contrarian_trades as f64) * 100.0
    } else {
        0.0
    };
    if contrarian_trades >= 5 && contrarian_rate >= 60.0 {
        labels.push(BehavioralLabel::Contrarian);
    }

    // Casual: small/infrequent
    if total_trade_count < 10 || total_volume < 500.0 {
        labels.push(BehavioralLabel::Casual);
    }

    let details = LabelDetails {
        win_rate,
        z_score,
        settled_count,
        dominant_category,
        dominant_category_pct: dominant_pct,
        category_win_rate: cat_win_rate,
        total_volume: format!("{:.6}", total_volume),
        avg_position_size_usd: format!("{:.6}", avg_position),
        unique_markets,
        total_trade_count,
        active_span_days,
        buy_sell_ratio,
        trades_per_market,
        contrarian_trades,
        contrarian_correct,
        contrarian_rate,
    };

    (labels, details)
}

// ---------------------------------------------------------------------------
// PolyLab Backtest
// ---------------------------------------------------------------------------

#[derive(clickhouse::Row, serde::Deserialize)]
struct TopTraderRow {
    address: String,
}

pub async fn backtest(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<BacktestRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Mutual-exclusion validation: exactly one of top_n or list_id
    if req.top_n.is_some() && req.list_id.is_some() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Specify list_id or top_n, not both".into(),
        ));
    }
    if req.top_n.is_none() && req.list_id.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Specify either list_id or top_n".into(),
        ));
    }

    let timeframe = match req.timeframe.as_str() {
        "7d" | "30d" | "all" => req.timeframe.as_str(),
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                "timeframe must be 7d, 30d, or all".into(),
            ));
        }
    };
    let initial_capital = req
        .initial_capital
        .unwrap_or(10_000.0)
        .clamp(100.0, 1_000_000.0);
    let copy_pct = req.copy_pct.unwrap_or(1.0).clamp(0.01, 1.0);

    // 1) Resolve trader addresses — from list or top-N
    let trader_rows: Vec<TopTraderRow>;

    if let Some(ref list_id) = req.list_id {
        let owner = user.0.clone();
        let addresses = {
            let conn = state.user_db.lock().unwrap_or_else(|p| p.into_inner());
            db::get_list_member_addresses(&conn, list_id, &owner).map_err(|e| match e {
                db::ListError::NotFound => (StatusCode::NOT_FOUND, "List not found".into()),
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to load list".into(),
                ),
            })?
        };
        if addresses.is_empty() {
            return Err((StatusCode::BAD_REQUEST, "List has no members".into()));
        }
        trader_rows = addresses
            .into_iter()
            .map(|a| TopTraderRow { address: a })
            .collect();
    } else {
        let top_n = req.top_n.unwrap().clamp(1, 50);
        let exclude = exclude_clause();
        let top_query = format!(
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
        trader_rows = state
            .db
            .query(&top_query)
            .fetch_all::<TopTraderRow>()
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    let top_n = trader_rows.len() as u32;
    let user_allocation = initial_capital * copy_pct;
    let per_trader_budget = if top_n > 0 {
        user_allocation / top_n as f64
    } else {
        0.0
    };

    let config = BacktestConfig {
        initial_capital,
        copy_pct,
        top_n,
        timeframe: timeframe.to_string(),
        per_trader_budget,
    };

    if trader_rows.is_empty() {
        return Ok(Json(BacktestResponse {
            portfolio_curve: vec![],
            pnl_curve: vec![],
            summary: BacktestSummary {
                total_pnl: "0.00".into(),
                total_return_pct: 0.0,
                win_rate: 0.0,
                max_drawdown: "0.00".into(),
                max_drawdown_pct: 0.0,
                positions_count: 0,
                traders_count: 0,
                initial_capital,
                final_value: initial_capital,
            },
            traders: vec![],
            config,
        }));
    }

    let addresses: Vec<String> = trader_rows
        .iter()
        .map(|r| r.address.to_lowercase())
        .collect();
    let in_list = addresses
        .iter()
        .map(|a| format!("'{a}'"))
        .collect::<Vec<_>>()
        .join(",");

    // 2) Fetch per-trader scaling data
    let scale_rows = state
        .db
        .query(&format!(
            "SELECT
            toString(p.trader) AS address,
            toString(ROUND(sum(p.buy_usdc) / count(), 6)) AS avg_position_size,
            count() AS market_count
        FROM poly_dearboard.trader_positions p
        WHERE lower(p.trader) IN ({in_list})
        GROUP BY p.trader"
        ))
        .fetch_all::<TraderScaleRow>()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut trader_scales: std::collections::HashMap<String, f64> =
        std::collections::HashMap::new();
    for row in &scale_rows {
        let avg_pos = row.avg_position_size.parse::<f64>().unwrap_or(1.0).max(1.0);
        let scale = per_trader_budget / avg_pos;
        trader_scales.insert(row.address.to_lowercase(), scale);
    }

    // 3) Build portfolio simulation from per-trader pnl_daily
    let day_filter = match timeframe {
        "7d" => Some(7),
        "30d" => Some(30),
        _ => None,
    };

    // Pre-window initial state (per-trader, for scaling)
    let mut asset_state: std::collections::HashMap<String, (f64, f64, f64)> =
        std::collections::HashMap::new();

    if let Some(days) = day_filter {
        let initial = state
            .db
            .query(&format!(
                "SELECT
                toString(trader) AS trader,
                asset_id,
                toString(sum(buy_amount) - sum(sell_amount)) AS net_tokens,
                toString(sum(sell_usdc) - sum(buy_usdc)) AS cash_flow,
                toString(argMaxMerge(last_price_state)) AS last_price
            FROM poly_dearboard.pnl_daily
            WHERE lower(trader) IN ({in_list})
              AND day < today() - {days}
            GROUP BY trader, asset_id"
            ))
            .fetch_all::<PnlInitialStateTraderRow>()
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        for row in initial {
            let scale = trader_scales
                .get(&row.trader.to_lowercase())
                .copied()
                .unwrap_or(1.0);
            let tokens = row.net_tokens.parse::<f64>().unwrap_or(0.0) * scale;
            let cash = row.cash_flow.parse::<f64>().unwrap_or(0.0) * scale;
            let price = row.last_price.parse::<f64>().unwrap_or(0.0);
            let entry = asset_state
                .entry(row.asset_id.clone())
                .or_insert((0.0, 0.0, 0.0));
            entry.0 += tokens;
            entry.1 += cash;
            entry.2 = price;
        }
    }

    // Window deltas (per-trader for scaling)
    let day_where = day_filter
        .map(|d| format!("AND day >= today() - {d}"))
        .unwrap_or_default();

    let rows = state
        .db
        .query(&format!(
            "SELECT
            toString(trader) AS trader,
            toString(day) AS date,
            asset_id,
            toString(sum(buy_amount) - sum(sell_amount)) AS net_token_delta,
            toString(sum(sell_usdc) - sum(buy_usdc)) AS cash_flow_delta,
            toString(argMaxMerge(last_price_state)) AS last_price
        FROM poly_dearboard.pnl_daily
        WHERE lower(trader) IN ({in_list})
          {day_where}
        GROUP BY trader, day, asset_id
        ORDER BY day, trader, asset_id"
        ))
        .fetch_all::<PnlDailyTraderRow>()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let resolved = fetch_resolved_prices(&state).await;

    // Simulate portfolio with scaling
    let portfolio_curve = simulate_portfolio(
        &rows,
        &mut asset_state,
        &resolved,
        &trader_scales,
        initial_capital,
    );

    // Also build raw PnL curve for backward compat
    let pnl_curve: Vec<PnlChartPoint> = portfolio_curve
        .iter()
        .map(|p| PnlChartPoint {
            date: p.date.clone(),
            pnl: p.pnl.clone(),
        })
        .collect();

    // 4) Summary stats
    let final_value = portfolio_curve
        .last()
        .and_then(|p| p.value.parse::<f64>().ok())
        .unwrap_or(initial_capital);
    let total_pnl = final_value - initial_capital;
    let total_return_pct = if initial_capital > 0.0 {
        (total_pnl / initial_capital) * 100.0
    } else {
        0.0
    };

    // Max drawdown on portfolio value
    let mut peak_value = initial_capital;
    let mut max_dd: f64 = 0.0;
    let mut max_dd_pct: f64 = 0.0;
    for pt in &portfolio_curve {
        let v = pt.value.parse::<f64>().unwrap_or(initial_capital);
        if v > peak_value {
            peak_value = v;
        }
        let dd = peak_value - v;
        if dd > max_dd {
            max_dd = dd;
        }
        let dd_pct = if peak_value > 0.0 {
            dd / peak_value * 100.0
        } else {
            0.0
        };
        if dd_pct > max_dd_pct {
            max_dd_pct = dd_pct;
        }
    }

    // Win rate + position count
    #[derive(clickhouse::Row, serde::Deserialize)]
    struct WinRateRow {
        total: u64,
        wins: u64,
    }
    let wr = state.db.query(&format!(
        "WITH resolved AS (
            SELECT asset_id, toNullable(toFloat64(resolved_price)) AS resolved_price
            FROM poly_dearboard.resolved_prices FINAL
        )
        SELECT
            count() AS total,
            countIf((p.sell_usdc - p.buy_usdc) + (p.buy_amount - p.sell_amount) * coalesce(rp.resolved_price, toFloat64(lp.latest_price)) > 0) AS wins
        FROM poly_dearboard.trader_positions p
        LEFT JOIN (SELECT asset_id, latest_price FROM poly_dearboard.asset_latest_price FINAL) AS lp ON p.asset_id = lp.asset_id
        LEFT JOIN resolved rp ON p.asset_id = rp.asset_id
        WHERE lower(p.trader) IN ({in_list})"
    ))
    .fetch_one::<WinRateRow>()
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let win_rate = if wr.total > 0 {
        (wr.wins as f64 / wr.total as f64) * 100.0
    } else {
        0.0
    };

    // 5) Per-trader breakdown with scaling
    #[derive(clickhouse::Row, serde::Deserialize)]
    struct TraderPnlRow {
        address: String,
        pnl: String,
        markets_traded: u64,
    }
    let trader_pnls = state.db.query(&format!(
        "WITH resolved AS (
            SELECT asset_id, toNullable(toFloat64(resolved_price)) AS resolved_price
            FROM poly_dearboard.resolved_prices FINAL
        )
        SELECT
            toString(p.trader) AS address,
            toString(ROUND(sum((p.sell_usdc - p.buy_usdc) + (p.buy_amount - p.sell_amount) * coalesce(rp.resolved_price, toFloat64(lp.latest_price))), 6)) AS pnl,
            count() AS markets_traded
        FROM poly_dearboard.trader_positions p
        LEFT JOIN (SELECT asset_id, latest_price FROM poly_dearboard.asset_latest_price FINAL) AS lp ON p.asset_id = lp.asset_id
        LEFT JOIN resolved rp ON p.asset_id = rp.asset_id
        WHERE lower(p.trader) IN ({in_list})
        GROUP BY p.trader
        ORDER BY sum((p.sell_usdc - p.buy_usdc) + (p.buy_amount - p.sell_amount) * coalesce(rp.resolved_price, toFloat64(lp.latest_price))) DESC"
    ))
    .fetch_all::<TraderPnlRow>()
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let total_scaled_abs: f64 = trader_pnls
        .iter()
        .map(|t| {
            let raw = t.pnl.parse::<f64>().unwrap_or(0.0);
            let scale = trader_scales
                .get(&t.address.to_lowercase())
                .copied()
                .unwrap_or(1.0);
            (raw * scale).abs()
        })
        .sum();

    let traders: Vec<BacktestTrader> = trader_pnls
        .into_iter()
        .enumerate()
        .map(|(i, t)| {
            let raw_pnl = t.pnl.parse::<f64>().unwrap_or(0.0);
            let scale = trader_scales
                .get(&t.address.to_lowercase())
                .copied()
                .unwrap_or(1.0);
            let scaled = raw_pnl * scale;
            BacktestTrader {
                address: t.address,
                rank: (i + 1) as u32,
                pnl: t.pnl,
                scaled_pnl: format!("{:.2}", scaled),
                markets_traded: t.markets_traded,
                contribution_pct: if total_scaled_abs > 0.0 {
                    (scaled.abs() / total_scaled_abs) * 100.0
                } else {
                    0.0
                },
                scale_factor: (scale * 1000.0).round() / 1000.0,
            }
        })
        .collect();

    Ok(Json(BacktestResponse {
        portfolio_curve,
        pnl_curve,
        summary: BacktestSummary {
            total_pnl: format!("{:.2}", total_pnl),
            total_return_pct: (total_return_pct * 10.0).round() / 10.0,
            win_rate: (win_rate * 10.0).round() / 10.0,
            max_drawdown: format!("{:.2}", max_dd),
            max_drawdown_pct: (max_dd_pct * 10.0).round() / 10.0,
            positions_count: wr.total,
            traders_count: top_n,
            initial_capital,
            final_value: (final_value * 100.0).round() / 100.0,
        },
        traders,
        config,
    }))
}

/// Portfolio simulation with per-trader scaling and capital constraints.
fn simulate_portfolio(
    rows: &[PnlDailyTraderRow],
    asset_state: &mut std::collections::HashMap<String, (f64, f64, f64)>,
    resolved: &std::collections::HashMap<String, f64>,
    trader_scales: &std::collections::HashMap<String, f64>,
    initial_capital: f64,
) -> Vec<PortfolioPoint> {
    // Compute initial cash: initial_capital minus cost of pre-window positions
    let pre_window_cost: f64 = asset_state
        .values()
        .map(|(_, cash, _)| -cash)
        .sum::<f64>()
        .max(0.0);
    let mut cash_balance = (initial_capital - pre_window_cost).max(0.0);

    let mut points: Vec<PortfolioPoint> = Vec::new();
    let mut current_date = String::new();

    for row in rows {
        if !current_date.is_empty() && row.date != current_date {
            // Emit point for previous date
            let positions_value: f64 = asset_state
                .values()
                .map(|(tokens, _, price)| tokens * price)
                .sum();
            let total_value = cash_balance + positions_value;
            let pnl = total_value - initial_capital;
            points.push(PortfolioPoint {
                date: current_date.clone(),
                value: format!("{:.2}", total_value),
                pnl: format!("{:.2}", pnl),
                pnl_pct: format!(
                    "{:.2}",
                    if initial_capital > 0.0 {
                        pnl / initial_capital * 100.0
                    } else {
                        0.0
                    }
                ),
            });
        }
        current_date.clone_from(&row.date);

        let scale = trader_scales
            .get(&row.trader.to_lowercase())
            .copied()
            .unwrap_or(1.0);
        let mut delta_tokens = row.net_token_delta.parse::<f64>().unwrap_or(0.0) * scale;
        let mut delta_cash = row.cash_flow_delta.parse::<f64>().unwrap_or(0.0) * scale;
        let price = row.last_price.parse::<f64>().unwrap_or(0.0);

        // Capital constraint: if buying (delta_cash < 0), clip to available cash
        if delta_cash < 0.0 {
            let cost = -delta_cash;
            if cost > cash_balance && cash_balance > 0.0 {
                let clip = cash_balance / cost;
                delta_tokens *= clip;
                delta_cash *= clip;
            } else if cash_balance <= 0.0 {
                // No cash left — skip this buy
                let entry = asset_state
                    .entry(row.asset_id.clone())
                    .or_insert((0.0, 0.0, 0.0));
                entry.2 = price; // Still update price
                continue;
            }
        }

        cash_balance += delta_cash;
        let entry = asset_state
            .entry(row.asset_id.clone())
            .or_insert((0.0, 0.0, 0.0));
        entry.0 += delta_tokens;
        entry.1 += delta_cash;
        entry.2 = price;
    }

    // Final point with resolved prices
    if !current_date.is_empty() {
        let positions_value: f64 = asset_state
            .iter()
            .map(|(asset_id, (tokens, _, price))| {
                let final_price = resolved.get(asset_id).copied().unwrap_or(*price);
                tokens * final_price
            })
            .sum();
        let total_value = cash_balance + positions_value;
        let pnl = total_value - initial_capital;
        points.push(PortfolioPoint {
            date: current_date,
            value: format!("{:.2}", total_value),
            pnl: format!("{:.2}", pnl),
            pnl_pct: format!(
                "{:.2}",
                if initial_capital > 0.0 {
                    pnl / initial_capital * 100.0
                } else {
                    0.0
                }
            ),
        });
    }

    points
}

// ---------------------------------------------------------------------------
// Copy Portfolio
// ---------------------------------------------------------------------------

pub async fn copy_portfolio(
    State(state): State<AppState>,
    user: AuthUser,
    Query(params): Query<CopyPortfolioParams>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Mutual exclusion: list_id and top cannot both be present
    if params.list_id.is_some() && params.top.is_some() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Specify list_id or top, not both".into(),
        ));
    }

    let (trader_filter, trader_count) = if let Some(ref list_id) = params.list_id {
        // List mode: load addresses from SQLite
        let owner = user.0.clone();
        let addresses = {
            let conn = state.user_db.lock().unwrap_or_else(|p| p.into_inner());
            db::get_list_member_addresses(&conn, list_id, &owner).map_err(|e| match e {
                db::ListError::NotFound => (StatusCode::NOT_FOUND, "List not found".into()),
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to load list".into(),
                ),
            })?
        };
        let count = addresses.len() as u32;
        let in_list = addresses
            .into_iter()
            .take(100)
            .map(|a| format!("'{a}'"))
            .collect::<Vec<_>>()
            .join(",");
        (in_list, count)
    } else {
        // Top-N mode (default)
        let top = params.top.unwrap_or(10).clamp(5, 50);
        (String::new(), top)
    };

    let query = if params.list_id.is_some() {
        // List mode: filter by explicit trader addresses (no ranking)
        format!(
            "WITH
                resolved AS (
                    SELECT asset_id, toNullable(toFloat64(resolved_price)) AS resolved_price
                    FROM poly_dearboard.resolved_prices FINAL
                )
            SELECT
                toString(p.trader) AS trader,
                p.asset_id AS asset_id,
                toString(p.buy_amount - p.sell_amount) AS net_tokens,
                toString(if(p.buy_amount > toDecimal128(0, 6),
                    ROUND(p.buy_usdc / p.buy_amount, 6),
                    toDecimal128(0, 6))) AS avg_entry,
                toString(toFloat64(lp.latest_price)) AS latest_price,
                toString(abs(toFloat64(p.buy_amount - p.sell_amount)) * toFloat64(lp.latest_price)) AS exposure,
                toString(ROUND((p.sell_usdc - p.buy_usdc) + (p.buy_amount - p.sell_amount) * toFloat64(lp.latest_price), 6)) AS pnl
            FROM poly_dearboard.trader_positions p
            LEFT JOIN (SELECT asset_id, latest_price FROM poly_dearboard.asset_latest_price FINAL) AS lp ON p.asset_id = lp.asset_id
            LEFT JOIN resolved rp ON p.asset_id = rp.asset_id
            WHERE p.trader IN ({trader_filter})
              AND rp.resolved_price IS NULL
              AND toFloat64(lp.latest_price) > 0.01
              AND toFloat64(lp.latest_price) < 0.99
              AND abs(p.buy_amount - p.sell_amount) > 0.01
            ORDER BY abs(toFloat64(p.buy_amount - p.sell_amount)) * toFloat64(lp.latest_price) DESC"
        )
    } else {
        // Top-N mode: use CTE to rank traders by PnL
        let top = trader_count;
        let exclude = exclude_clause();
        format!(
            "WITH
                resolved AS (
                    SELECT asset_id, toNullable(toFloat64(resolved_price)) AS resolved_price
                    FROM poly_dearboard.resolved_prices FINAL
                ),
                trader_pnl AS (
                    SELECT p.trader,
                           sum((p.sell_usdc - p.buy_usdc) + (p.buy_amount - p.sell_amount) * coalesce(rp.resolved_price, toFloat64(lp.latest_price))) AS total_pnl
                    FROM poly_dearboard.trader_positions p
                    LEFT JOIN (SELECT asset_id, latest_price FROM poly_dearboard.asset_latest_price FINAL) AS lp ON p.asset_id = lp.asset_id
                    LEFT JOIN resolved rp ON p.asset_id = rp.asset_id
                    WHERE p.trader NOT IN ({exclude})
                    GROUP BY p.trader
                    ORDER BY total_pnl DESC
                    LIMIT {top}
                )
            SELECT
                toString(p.trader) AS trader,
                p.asset_id AS asset_id,
                toString(p.buy_amount - p.sell_amount) AS net_tokens,
                toString(if(p.buy_amount > toDecimal128(0, 6),
                    ROUND(p.buy_usdc / p.buy_amount, 6),
                    toDecimal128(0, 6))) AS avg_entry,
                toString(toFloat64(lp.latest_price)) AS latest_price,
                toString(abs(toFloat64(p.buy_amount - p.sell_amount)) * toFloat64(lp.latest_price)) AS exposure,
                toString(ROUND((p.sell_usdc - p.buy_usdc) + (p.buy_amount - p.sell_amount) * toFloat64(lp.latest_price), 6)) AS pnl
            FROM poly_dearboard.trader_positions p
            LEFT JOIN (SELECT asset_id, latest_price FROM poly_dearboard.asset_latest_price FINAL) AS lp ON p.asset_id = lp.asset_id
            LEFT JOIN resolved rp ON p.asset_id = rp.asset_id
            WHERE p.trader IN (SELECT trader FROM trader_pnl)
              AND rp.resolved_price IS NULL
              AND toFloat64(lp.latest_price) > 0.01
              AND toFloat64(lp.latest_price) < 0.99
              AND abs(p.buy_amount - p.sell_amount) > 0.01
            ORDER BY abs(toFloat64(p.buy_amount - p.sell_amount)) * toFloat64(lp.latest_price) DESC"
        )
    };

    let rows = state
        .db
        .query(&query)
        .fetch_all::<CopyPortfolioRow>()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Market enrichment
    let asset_ids: Vec<String> = rows.iter().map(|r| r.asset_id.clone()).collect();
    let market_info =
        markets::resolve_markets(&state.http, &state.db, &state.market_cache, &asset_ids).await;

    // Merge Yes/No tokens of the same market, aggregate per question
    let mut merged: std::collections::HashMap<String, CopyPortfolioPosition> =
        std::collections::HashMap::new();
    // Track unique traders per market for convergence count
    let mut traders_per_market: std::collections::HashMap<
        String,
        std::collections::HashSet<String>,
    > = std::collections::HashMap::new();

    for r in &rows {
        let info = match market_info.get(&r.asset_id) {
            Some(i) if i.active => i,
            _ => continue,
        };

        let question = info.question.clone();
        let net: f64 = r.net_tokens.parse().unwrap_or(0.0);
        let exposure: f64 = r.exposure.parse().unwrap_or(0.0);
        let pnl: f64 = r.pnl.parse().unwrap_or(0.0);
        let entry: f64 = r.avg_entry.parse().unwrap_or(0.0);
        let is_long = net > 0.0;

        let traders = traders_per_market.entry(question.clone()).or_default();
        let is_new_trader = traders.insert(r.trader.clone());

        if let Some(existing) = merged.get_mut(&question) {
            let ex_exp: f64 = existing.total_exposure.parse().unwrap_or(0.0);
            let ex_pnl: f64 = existing.total_pnl.parse().unwrap_or(0.0);
            let ex_entry: f64 = existing.avg_entry.parse().unwrap_or(0.0);

            let new_total_exp = ex_exp + exposure;
            let weighted_entry = if new_total_exp > 0.0 {
                (ex_exp * ex_entry + exposure * entry) / new_total_exp
            } else {
                0.0
            };

            existing.total_exposure = format!("{new_total_exp:.6}");
            existing.total_pnl = format!("{:.6}", ex_pnl + pnl);
            existing.avg_entry = format!("{weighted_entry:.6}");
            // Only count unique traders for convergence
            if is_new_trader {
                if is_long {
                    existing.long_count += 1;
                } else {
                    existing.short_count += 1;
                }
            }
        } else {
            merged.insert(
                question.clone(),
                CopyPortfolioPosition {
                    token_id: info.gamma_token_id.clone(),
                    question,
                    outcome: info.outcome.clone(),
                    convergence: 0, // set from HashSet len after loop
                    long_count: if is_long { 1 } else { 0 },
                    short_count: if !is_long { 1 } else { 0 },
                    total_exposure: format!("{exposure:.6}"),
                    avg_entry: format!("{entry:.6}"),
                    latest_price: r.latest_price.clone(),
                    total_pnl: format!("{pnl:.6}"),
                },
            );
        }
    }

    // Set convergence from unique trader counts
    for (question, pos) in merged.iter_mut() {
        if let Some(traders) = traders_per_market.get(question) {
            pos.convergence = traders.len() as u32;
        }
    }

    // Sort by convergence DESC, then exposure DESC
    let mut positions: Vec<CopyPortfolioPosition> = merged.into_values().collect();
    positions.sort_by(|a, b| {
        b.convergence.cmp(&a.convergence).then_with(|| {
            let a_exp: f64 = a.total_exposure.parse().unwrap_or(0.0);
            let b_exp: f64 = b.total_exposure.parse().unwrap_or(0.0);
            b_exp
                .partial_cmp(&a_exp)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });

    let total_exposure: f64 = positions
        .iter()
        .map(|p| p.total_exposure.parse::<f64>().unwrap_or(0.0))
        .sum();
    let total_pnl: f64 = positions
        .iter()
        .map(|p| p.total_pnl.parse::<f64>().unwrap_or(0.0))
        .sum();

    let summary = CopyPortfolioSummary {
        total_positions: positions.len() as u32,
        unique_markets: positions.len() as u32,
        total_exposure: format!("{total_exposure:.6}"),
        total_pnl: format!("{total_pnl:.6}"),
        top_n: trader_count,
    };

    Ok(Json(CopyPortfolioResponse { positions, summary }))
}

fn shorten_id(id: &str) -> String {
    if id.len() <= 12 {
        id.to_string()
    } else {
        format!("{}...{}", &id[..6], &id[id.len() - 4..])
    }
}

// ---------------------------------------------------------------------------
// Trader Lists CRUD
// ---------------------------------------------------------------------------

fn map_list_error(e: db::ListError) -> (StatusCode, String) {
    match e {
        db::ListError::LimitExceeded(msg) => (StatusCode::BAD_REQUEST, msg.into()),
        db::ListError::DuplicateName => (
            StatusCode::CONFLICT,
            "A list with this name already exists".into(),
        ),
        db::ListError::NotFound => (StatusCode::NOT_FOUND, "List not found".into()),
        db::ListError::Db(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub async fn list_trader_lists(
    State(state): State<AppState>,
    AuthUser(owner): AuthUser,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let conn = state.user_db.lock().unwrap_or_else(|p| p.into_inner());
    let lists = db::list_trader_lists(&conn, &owner)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(lists))
}

pub async fn create_trader_list(
    State(state): State<AppState>,
    AuthUser(owner): AuthUser,
    Json(req): Json<CreateListRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let name = req.name.trim().to_string();
    if name.is_empty() || name.len() > 50 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Name must be 1-50 characters".into(),
        ));
    }
    let conn = state.user_db.lock().unwrap_or_else(|p| p.into_inner());
    let list = db::create_trader_list(&conn, &owner, &name).map_err(map_list_error)?;
    Ok((StatusCode::CREATED, Json(list)))
}

pub async fn get_trader_list(
    State(state): State<AppState>,
    AuthUser(owner): AuthUser,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let conn = state.user_db.lock().unwrap_or_else(|p| p.into_inner());
    let detail = db::get_trader_list(&conn, &id, &owner).map_err(map_list_error)?;
    Ok(Json(detail))
}

pub async fn rename_trader_list(
    State(state): State<AppState>,
    AuthUser(owner): AuthUser,
    Path(id): Path<String>,
    Json(req): Json<RenameListRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let name = req.name.trim().to_string();
    if name.is_empty() || name.len() > 50 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Name must be 1-50 characters".into(),
        ));
    }
    let conn = state.user_db.lock().unwrap_or_else(|p| p.into_inner());
    db::rename_trader_list(&conn, &id, &owner, &name).map_err(map_list_error)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_trader_list(
    State(state): State<AppState>,
    AuthUser(owner): AuthUser,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let conn = state.user_db.lock().unwrap_or_else(|p| p.into_inner());
    db::delete_trader_list(&conn, &id, &owner).map_err(map_list_error)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn add_list_members(
    State(state): State<AppState>,
    AuthUser(owner): AuthUser,
    Path(id): Path<String>,
    Json(req): Json<AddMembersRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    if req.addresses.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "At least one address required".into(),
        ));
    }

    let labels = req.labels.unwrap_or_default();

    let members: Vec<(String, Option<String>)> = req
        .addresses
        .iter()
        .enumerate()
        .map(|(i, addr)| {
            let validated = middleware::validate_eth_address(addr)
                .map_err(|_| (StatusCode::BAD_REQUEST, format!("Invalid address: {addr}")))?;
            let label = labels.get(i).and_then(|l| l.clone());
            Ok((validated, label))
        })
        .collect::<Result<Vec<_>, (StatusCode, String)>>()?;

    let conn = state.user_db.lock().unwrap_or_else(|p| p.into_inner());
    db::add_list_members(&conn, &id, &owner, &members).map_err(map_list_error)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn remove_list_members(
    State(state): State<AppState>,
    AuthUser(owner): AuthUser,
    Path(id): Path<String>,
    Json(req): Json<RemoveMembersRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let addresses: Vec<String> = req.addresses.iter().map(|a| a.to_lowercase()).collect();

    let conn = state.user_db.lock().unwrap_or_else(|p| p.into_inner());
    db::remove_list_members(&conn, &id, &owner, &addresses).map_err(map_list_error)?;
    Ok(StatusCode::NO_CONTENT)
}
