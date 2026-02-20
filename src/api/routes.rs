use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};

use super::markets;
use super::server::AppState;
use super::types::*;

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

fn exclude_clause() -> String {
    EXCHANGE_CONTRACTS
        .iter()
        .map(|a| format!("'{a}'"))
        .collect::<Vec<_>>()
        .join(",")
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
            "realized_pnl" => "sum((p.sell_usdc - p.buy_usdc) + (p.buy_amount - p.sell_amount) * coalesce(rp.resolved_price, toFloat64(lp.latest_price)))",
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
            LEFT JOIN (SELECT * FROM poly_dearboard.asset_latest_price FINAL) AS lp ON p.asset_id = lp.asset_id
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
        let time_filter = match timeframe {
            "1h" => "AND block_timestamp >= now() - INTERVAL 1 HOUR",
            "24h" => "AND block_timestamp >= now() - INTERVAL 24 HOUR",
            _ => "",
        };

        let sort_expr = match sort {
            "realized_pnl" => "sum(p.cash_flow + p.net_tokens * coalesce(rp.resolved_price, toFloat64(lp.latest_price)))",
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
                    WHERE trader NOT IN ({exclude}) {time_filter}
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
            LEFT JOIN (SELECT * FROM poly_dearboard.asset_latest_price FINAL) AS lp ON p.asset_id = lp.asset_id
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
                "SELECT uniqExact(trader) FROM poly_dearboard.trades WHERE trader NOT IN ({exclude}) {time_filter}"
            ))
            .fetch_one()
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        (traders, total)
    };

    Ok(Json(LeaderboardResponse {
        traders,
        total,
        limit,
        offset,
    }))
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
            LEFT JOIN (SELECT * FROM poly_dearboard.asset_latest_price FINAL) AS lp ON p.asset_id = lp.asset_id
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
            markets::resolve_markets(&state.http, &state.market_cache, &token_ids).await;
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
                LIMIT ?"
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
            WHERE block_timestamp >= now() - INTERVAL {interval}
              AND trader NOT IN ({exclude})
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
        markets::resolve_markets(&state.http, &state.market_cache, &token_ids).await;

    // Merge tokens belonging to the same event (Yes/No → one row)
    let mut merged: std::collections::HashMap<String, HotMarket> =
        std::collections::HashMap::new();

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
        markets::resolve_markets(&state.http, &state.market_cache, &token_ids).await;

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
            FROM poly_dearboard.global_stats"
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
            LEFT JOIN (SELECT * FROM poly_dearboard.asset_latest_price FINAL) AS lp ON p.asset_id = lp.asset_id
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
        markets::resolve_markets(&state.http, &state.market_cache, &token_ids).await;

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
            WHERE lower(trader) = ?
              AND block_timestamp > toDateTime('1970-01-01 00:00:00')
              AND block_timestamp < now() - INTERVAL 24 HOUR
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
            WHERE lower(trader) = ?
              AND block_timestamp > toDateTime('1970-01-01 00:00:00')
              AND block_timestamp >= now() - INTERVAL 24 HOUR
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
        .filter_map(|r| r.resolved_price.parse::<f64>().ok().map(|p| (r.asset_id, p)))
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

    let info = markets::resolve_markets(&state.http, &state.market_cache, &token_ids).await;

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

pub async fn verify_access_code(
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let expected = std::env::var("ACCESS_CODE").unwrap_or_default();
    if expected.is_empty() {
        return StatusCode::OK;
    }
    let provided = body.get("code").and_then(|v| v.as_str()).unwrap_or("");
    if provided == expected {
        StatusCode::OK
    } else {
        StatusCode::UNAUTHORIZED
    }
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
                    LEFT JOIN (SELECT * FROM poly_dearboard.asset_latest_price FINAL) AS lp ON p.asset_id = lp.asset_id
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
                    LEFT JOIN (SELECT * FROM poly_dearboard.asset_latest_price FINAL) AS lp ON p.asset_id = lp.asset_id
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
        let time_filter = match timeframe {
            "1h" => "AND block_timestamp >= now() - INTERVAL 1 HOUR",
            "24h" => "AND block_timestamp >= now() - INTERVAL 24 HOUR",
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
                        WHERE trader NOT IN ({exclude})
                          {time_filter}
                        GROUP BY trader, asset_id
                    ) p
                    LEFT JOIN (SELECT * FROM poly_dearboard.asset_latest_price FINAL) AS lp ON p.asset_id = lp.asset_id
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
                    LEFT JOIN (SELECT * FROM poly_dearboard.asset_latest_price FINAL) AS lp ON p.asset_id = lp.asset_id
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
        markets::resolve_markets(&state.http, &state.market_cache, &token_ids).await;

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

fn shorten_id(id: &str) -> String {
    if id.len() <= 12 {
        id.to_string()
    } else {
        format!("{}...{}", &id[..6], &id[id.len() - 4..])
    }
}
