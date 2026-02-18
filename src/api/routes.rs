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
        "realized_pnl" => "sum(p.cash_flow + p.net_tokens * lp.latest_price)",
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
            toString(ROUND(sum(p.cash_flow + p.net_tokens * lp.latest_price), 6)) AS realized_pnl,
            toString(sum(p.fees)) AS total_fees,
            ifNull(toString(min(p.first_ts)), '') AS first_trade,
            ifNull(toString(max(p.last_ts)), '') AS last_trade
        FROM positions p
        LEFT JOIN latest_prices lp ON p.asset_id = lp.asset_id
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
                toString(ROUND(sum(p.cash_flow + p.net_tokens * lp.latest_price), 6)) AS realized_pnl,
                toString(sum(p.fees)) AS total_fees,
                ifNull(toString(min(p.first_ts)), '') AS first_trade,
                ifNull(toString(max(p.last_ts)), '') AS last_trade
            FROM positions p
            LEFT JOIN latest_prices lp ON p.asset_id = lp.asset_id
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

    // Convert any format (full-precision integers, scientific notation) to
    // ClickHouse's stored format (f64 scientific notation) for exact matching.
    let token_ids: Vec<String> = token_ids
        .iter()
        .map(|id| markets::to_clickhouse_id(id))
        .collect();

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
            "WITH latest_prices AS (
                SELECT asset_id,
                       argMax(price, block_number * 1000000 + log_index) AS latest_price
                FROM poly_dearboard.trades
                GROUP BY asset_id
            )
            SELECT
                p.asset_id,
                p.side_summary,
                toString(p.net_tokens) AS net_tokens,
                toString(p.cost_basis) AS cost_basis,
                toString(lp.latest_price) AS latest_price,
                toString(ROUND(p.cash_flow + p.net_tokens * lp.latest_price, 6)) AS pnl,
                toString(p.volume) AS volume,
                p.trades AS trade_count
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
            ORDER BY abs(p.net_tokens * lp.latest_price) DESC",
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
        let settled = api_resolved || price_settled || user_exited;

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
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let address = address.to_lowercase();

    // Daily cash flow: sells - buys. Running sum computed in Rust to avoid
    // clickhouse crate issues with window functions in binary protocol.
    let rows = state
        .db
        .query(
            "SELECT
                ifNull(toString(toDate(block_timestamp)), '') AS date,
                toString(ROUND(sum(if(side = 'sell', toFloat64(usdc_amount), -toFloat64(usdc_amount))), 2)) AS pnl
            FROM poly_dearboard.trades
            WHERE lower(trader) = ?
              AND block_timestamp > toDateTime('1970-01-01 00:00:00')
            GROUP BY toDate(block_timestamp)
            ORDER BY toDate(block_timestamp)",
        )
        .bind(&address)
        .fetch_all::<PnlChartRow>()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Cumulative sum
    let mut cumulative = 0.0;
    let points = rows
        .into_iter()
        .map(|r| {
            cumulative += r.pnl.parse::<f64>().unwrap_or(0.0);
            PnlChartPoint {
                date: r.date,
                pnl: format!("{:.2}", cumulative),
            }
        })
        .collect();

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

fn shorten_id(id: &str) -> String {
    if id.len() <= 12 {
        id.to_string()
    } else {
        format!("{}...{}", &id[..6], &id[id.len() - 4..])
    }
}
