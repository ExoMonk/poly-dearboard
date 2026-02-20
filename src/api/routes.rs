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

    let sort_expr = match sort {
        "realized_pnl" => {
            "sum(p.cash_flow + p.net_tokens * coalesce(rp.resolved_price, toFloat64(lp.latest_price)))"
        }
        "total_volume" => "sum(p.volume)",
        "trade_count" => "sum(p.trades)",
        _ => unreachable!(),
    };

    let time_filter = match params.timeframe.as_deref().unwrap_or("all") {
        "1h" => "AND block_timestamp >= now() - INTERVAL 1 HOUR",
        "24h" => "AND block_timestamp >= now() - INTERVAL 24 HOUR",
        _ => "",
    };

    let exclude = exclude_clause();

    let query = format!(
        "WITH
            latest_prices AS (
                SELECT asset_id,
                       argMax(price, block_number * 1000000 + log_index) AS latest_price
                FROM poly_dearboard.trades
                GROUP BY asset_id
            ),
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
        LEFT JOIN latest_prices lp ON p.asset_id = lp.asset_id
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
            "WITH
                latest_prices AS (
                    SELECT asset_id,
                           argMax(price, block_number * 1000000 + log_index) AS latest_price
                    FROM poly_dearboard.trades
                    GROUP BY asset_id
                ),
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
                    WHERE lower(trader) = ?
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
            LEFT JOIN latest_prices lp ON p.asset_id = lp.asset_id
            LEFT JOIN resolved rp ON p.asset_id = rp.asset_id
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
    let interval = match params.period.as_deref().unwrap_or("24h") {
        "1h" => "1 HOUR",
        "7d" => "7 DAY",
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

    // Fetch extra rows since Yes/No tokens will be merged into one event
    let fetch_limit = limit * 3;

    let rows = state
        .db
        .query(&query)
        .bind(fetch_limit)
        .fetch_all::<MarketStatsRow>()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

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
    let exclude = exclude_clause();

    let stats = state
        .db
        .query(&format!(
            "SELECT
                count() AS trade_count,
                uniqExact(trader) AS trader_count,
                max(block_number) AS latest_block
            FROM poly_dearboard.trades
            WHERE trader NOT IN ({exclude})"
        ))
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
            "WITH
                latest_prices AS (
                    SELECT asset_id,
                           argMax(price, block_number * 1000000 + log_index) AS latest_price
                    FROM poly_dearboard.trades
                    GROUP BY asset_id
                ),
                resolved AS (
                    SELECT asset_id, toNullable(toFloat64(resolved_price)) AS resolved_price
                    FROM poly_dearboard.resolved_prices FINAL
                )
            SELECT
                p.asset_id,
                p.side_summary,
                toString(p.net_tokens) AS net_tokens,
                toString(p.cost_basis) AS cost_basis,
                toString(coalesce(rp.resolved_price, toFloat64(lp.latest_price))) AS latest_price,
                toString(ROUND(p.cash_flow + p.net_tokens * coalesce(rp.resolved_price, toFloat64(lp.latest_price)), 6)) AS pnl,
                toString(p.volume) AS volume,
                p.trades AS trade_count,
                if(rp.resolved_price IS NOT NULL, 1, 0) AS on_chain_resolved
            FROM (
                SELECT asset_id,
                       sumIf(amount, side = 'buy') - sumIf(amount, side = 'sell') AS net_tokens,
                       sumIf(usdc_amount, side = 'sell') - sumIf(usdc_amount, side = 'buy') AS cash_flow,
                       if(sumIf(amount, side = 'buy') > 0,
                          sumIf(usdc_amount, side = 'buy') / sumIf(amount, side = 'buy'),
                          toDecimal128(0, 6)) AS cost_basis,
                       if(sumIf(amount, side = 'buy') > sumIf(amount, side = 'sell'), 'long',
                          if(sumIf(amount, side = 'sell') > sumIf(amount, side = 'buy'), 'short', 'closed')) AS side_summary,
                       sum(usdc_amount) AS volume,
                       count() AS trades
                FROM poly_dearboard.trades
                WHERE lower(trader) = ?
                GROUP BY asset_id
            ) p
            LEFT JOIN latest_prices lp ON p.asset_id = lp.asset_id
            LEFT JOIN resolved rp ON p.asset_id = rp.asset_id
            ORDER BY abs(p.net_tokens * coalesce(rp.resolved_price, toFloat64(lp.latest_price))) DESC",
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

    // Granularity + window based on timeframe
    let (bucket_expr, date_format, window_interval) = match timeframe {
        "24h" => (
            "toStartOfHour(block_timestamp)",
            "ifNull(toString(toStartOfHour(block_timestamp)), '')",
            Some("24 HOUR"),
        ),
        "7d" => (
            "toDate(block_timestamp)",
            "ifNull(toString(toDate(block_timestamp)), '')",
            Some("7 DAY"),
        ),
        "30d" => (
            "toDate(block_timestamp)",
            "ifNull(toString(toDate(block_timestamp)), '')",
            Some("30 DAY"),
        ),
        _ => (
            "toDate(block_timestamp)",
            "ifNull(toString(toDate(block_timestamp)), '')",
            None,
        ),
    };

    // For windowed views: compute initial portfolio state from trades before the window
    let mut asset_state: std::collections::HashMap<String, (f64, f64, f64)> =
        std::collections::HashMap::new();

    if let Some(interval) = window_interval {
        let initial = state
            .db
            .query(&format!(
                "SELECT
                    asset_id,
                    toString(sumIf(toFloat64(amount), side='buy') - sumIf(toFloat64(amount), side='sell')) AS net_tokens,
                    toString(sumIf(toFloat64(usdc_amount), side='sell') - sumIf(toFloat64(usdc_amount), side='buy')) AS cash_flow,
                    toString(argMax(toFloat64(price), block_number * 1000000 + log_index)) AS last_price
                FROM poly_dearboard.trades
                WHERE lower(trader) = ?
                  AND block_timestamp > toDateTime('1970-01-01 00:00:00')
                  AND block_timestamp < now() - INTERVAL {interval}
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

    // Fetch per-(bucket, asset) trade summaries within the window
    let window_filter = window_interval
        .map(|i| format!("AND block_timestamp >= now() - INTERVAL {i}"))
        .unwrap_or_default();

    let rows = state
        .db
        .query(&format!(
            "SELECT
                {date_format} AS date,
                asset_id,
                toString(sumIf(toFloat64(amount), side = 'buy') - sumIf(toFloat64(amount), side = 'sell')) AS net_token_delta,
                toString(sumIf(toFloat64(usdc_amount), side = 'sell') - sumIf(toFloat64(usdc_amount), side = 'buy')) AS cash_flow_delta,
                toString(argMax(toFloat64(price), block_number * 1000000 + log_index)) AS last_price
            FROM poly_dearboard.trades
            WHERE lower(trader) = ?
              AND block_timestamp > toDateTime('1970-01-01 00:00:00')
              {window_filter}
            GROUP BY {bucket_expr}, asset_id
            ORDER BY {bucket_expr}, asset_id"
        ))
        .bind(&address)
        .fetch_all::<PnlDailyRow>()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if rows.is_empty() && asset_state.is_empty() {
        return Ok(Json(PnlChartResponse { points: vec![] }));
    }

    // Resolved prices for final-point overlay (matches leaderboard COALESCE logic)
    let resolved: std::collections::HashMap<String, f64> = state
        .db
        .query("SELECT asset_id, resolved_price FROM poly_dearboard.resolved_prices FINAL")
        .fetch_all::<ResolvedPriceLookup>()
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|r| r.resolved_price.parse::<f64>().ok().map(|p| (r.asset_id, p)))
        .collect();

    // Process bucket-by-bucket, maintaining per-asset running state:
    // (cumulative_net_tokens, cumulative_cash_flow, last_known_price)
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

    Ok(Json(PnlChartResponse { points }))
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
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let exclude = exclude_clause();

    let query = format!(
        "WITH
            latest_prices AS (
                SELECT asset_id,
                       argMax(price, block_number * 1000000 + log_index) AS latest_price
                FROM poly_dearboard.trades
                GROUP BY asset_id
            ),
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
                    GROUP BY trader, asset_id
                ) p
                LEFT JOIN latest_prices lp ON p.asset_id = lp.asset_id
                LEFT JOIN resolved rp ON p.asset_id = rp.asset_id
                GROUP BY trader
                ORDER BY total_pnl DESC
                LIMIT 10
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
                LEFT JOIN latest_prices lp ON p.asset_id = lp.asset_id
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

    let rows = state
        .db
        .query(&query)
        .fetch_all::<SmartMoneyRow>()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let token_ids: Vec<String> = rows.iter().map(|r| r.asset_id.clone()).collect();
    let market_info =
        markets::resolve_markets(&state.http, &state.market_cache, &token_ids).await;

    // Merge Yes/No tokens of the same market into one entry
    let mut merged: std::collections::HashMap<String, SmartMoneyMarket> =
        std::collections::HashMap::new();

    for r in rows {
        let info = market_info.get(&r.asset_id);

        // Skip resolved/inactive markets (API knows before on-chain settlement)
        if info.map(|i| !i.active).unwrap_or(false) {
            continue;
        }

        let question = info
            .map(|i| i.question.clone())
            .unwrap_or_else(|| shorten_id(&r.asset_id));
        let token_id = info
            .map(|i| i.gamma_token_id.clone())
            .unwrap_or_else(|| markets::to_integer_id(&r.asset_id));
        let outcome = info.map(|i| i.outcome.clone()).unwrap_or_default();

        let long_exp: f64 = r.long_exposure.parse().unwrap_or(0.0);
        let short_exp: f64 = r.short_exposure.parse().unwrap_or(0.0);

        if let Some(existing) = merged.get_mut(&question) {
            let existing_long: f64 = existing.long_exposure.parse().unwrap_or(0.0);
            let existing_short: f64 = existing.short_exposure.parse().unwrap_or(0.0);
            existing.long_exposure = format!("{:.6}", existing_long + long_exp);
            existing.short_exposure = format!("{:.6}", existing_short + short_exp);
            existing.long_count += r.long_count;
            existing.short_count += r.short_count;
            // smart_trader_count: take max since same traders may hold both tokens
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

    Ok(Json(SmartMoneyResponse { markets }))
}

fn shorten_id(id: &str) -> String {
    if id.len() <= 12 {
        id.to_string()
    } else {
        format!("{}...{}", &id[..6], &id[id.len() - 4..])
    }
}
