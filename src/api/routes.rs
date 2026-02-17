use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};

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

pub async fn leaderboard(
    State(client): State<clickhouse::Client>,
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

    // Map API sort names to numeric ClickHouse expressions for proper ordering
    // Note: fee is 0 in maker-only MVs (fees tracked separately if needed)
    let sort_expr = match sort {
        "realized_pnl" => "sumIf(usdc_amount, side = 'sell') - sumIf(usdc_amount, side = 'buy')",
        "total_volume" => "sum(usdc_amount)",
        "trade_count" => "count()",
        _ => unreachable!(),
    };

    let exclude = EXCHANGE_CONTRACTS.iter().map(|a| format!("'{a}'")).collect::<Vec<_>>().join(",");

    let query = format!(
        "SELECT
            toString(trader) AS address,
            toString(sum(usdc_amount)) AS total_volume,
            count() AS trade_count,
            uniqExact(asset_id) AS markets_traded,
            toString(sumIf(usdc_amount, side = 'sell') - sumIf(usdc_amount, side = 'buy')) AS realized_pnl,
            toString(sum(fee)) AS total_fees,
            ifNull(toString(min(block_timestamp)), '') AS first_trade,
            ifNull(toString(max(block_timestamp)), '') AS last_trade
        FROM poly_dearboard.trades
        WHERE trader NOT IN ({exclude})
        GROUP BY trader
        ORDER BY {sort_expr} {order}
        LIMIT ? OFFSET ?"
    );

    let traders = client
        .query(&query)
        .bind(limit)
        .bind(offset)
        .fetch_all::<TraderSummary>()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let total: u64 = client
        .query(&format!("SELECT uniqExact(trader) FROM poly_dearboard.trades WHERE trader NOT IN ({exclude})"))
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
    State(client): State<clickhouse::Client>,
    Path(address): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let address = address.to_lowercase();

    let result = client
        .query(
            "SELECT
                toString(trader) AS address,
                toString(sum(usdc_amount)) AS total_volume,
                count() AS trade_count,
                uniqExact(asset_id) AS markets_traded,
                toString(sumIf(usdc_amount, side = 'sell') - sumIf(usdc_amount, side = 'buy')) AS realized_pnl,
                toString(sum(fee)) AS total_fees,
                ifNull(toString(min(block_timestamp)), '') AS first_trade,
                ifNull(toString(max(block_timestamp)), '') AS last_trade
            FROM poly_dearboard.trades
            WHERE lower(trader) = ?
            GROUP BY trader",
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
    State(client): State<clickhouse::Client>,
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

    let trades = client
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

    let total: u64 = client
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

pub async fn health(
    State(client): State<clickhouse::Client>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let exclude = EXCHANGE_CONTRACTS.iter().map(|a| format!("'{a}'")).collect::<Vec<_>>().join(",");

    let stats = client
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
