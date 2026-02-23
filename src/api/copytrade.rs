use axum::extract::{Json, Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;

use super::db::{self, CopyTradeSessionRow};
use super::engine::CopyTradeCommand;
use super::middleware::AuthUser;
use super::server::AppState;
use super::types::{
    ClosePositionRequest, CopyOrderType, CopyTradeOrder, CopyTradeOrderSummary, CopyTradePosition,
    CopyTradeSession, CopyTradeSummary, CopyTradeUpdate, CreateSessionRequest, OrderStatus,
    SessionOrdersParams, SessionPatchRequest, SessionStats, SessionStatus,
};

// ---------------------------------------------------------------------------
// POST /api/copytrade/sessions
// ---------------------------------------------------------------------------

pub async fn create_session(
    State(state): State<AppState>,
    AuthUser(owner): AuthUser,
    Json(req): Json<CreateSessionRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Validate config
    if req.copy_pct < 0.05 || req.copy_pct > 1.0 {
        return Err((
            StatusCode::BAD_REQUEST,
            "copy_pct must be between 0.05 and 1.0".into(),
        ));
    }
    if req.initial_capital <= 0.0 {
        return Err((
            StatusCode::BAD_REQUEST,
            "initial_capital must be positive".into(),
        ));
    }
    if req.max_position_usdc <= 0.0 {
        return Err((
            StatusCode::BAD_REQUEST,
            "max_position_usdc must be positive".into(),
        ));
    }
    if req.list_id.is_some() && req.top_n.is_some() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Specify list_id or top_n, not both".into(),
        ));
    }
    if req.list_id.is_none() && req.top_n.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Specify either list_id or top_n".into(),
        ));
    }
    if CopyOrderType::from_str(&req.order_type).is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            "order_type must be FOK or GTC".into(),
        ));
    }

    // If not simulation, require funded wallet with CLOB credentials
    if !req.simulate {
        let wallets = {
            let conn = state.user_db.lock().unwrap_or_else(|p| p.into_inner());
            db::get_trading_wallets(&conn, &owner)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        };
        let has_credentialed = wallets.iter().any(|w| w.clob_api_key.is_some());
        if !has_credentialed {
            return Err((
                StatusCode::BAD_REQUEST,
                "No wallet with CLOB credentials. Derive credentials first.".into(),
            ));
        }
    }

    // Create session
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let order_type_str = CopyOrderType::from_str(&req.order_type)
        .unwrap_or(CopyOrderType::FOK)
        .as_str()
        .to_string();

    let row = CopyTradeSessionRow {
        id: id.clone(),
        owner: owner.clone(),
        list_id: req.list_id.clone(),
        top_n: req.top_n,
        copy_pct: req.copy_pct,
        max_position_usdc: req.max_position_usdc,
        max_slippage_bps: req.max_slippage_bps,
        order_type: order_type_str,
        initial_capital: req.initial_capital,
        remaining_capital: req.initial_capital,
        simulate: req.simulate,
        max_loss_pct: req.max_loss_pct,
        status: "running".to_string(),
        created_at: now.clone(),
        updated_at: now,
    };

    {
        let conn = state.user_db.lock().unwrap_or_else(|p| p.into_inner());
        db::create_copytrade_session(&conn, &row)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    // Send Start command to engine
    let _ = state
        .copytrade_cmd_tx
        .send(CopyTradeCommand::Start {
            session_id: id.clone(),
            owner: owner.clone(),
        })
        .await;

    Ok(Json(session_from_row(&row, 0.0))) // New session, no positions yet
}

// ---------------------------------------------------------------------------
// GET /api/copytrade/sessions
// ---------------------------------------------------------------------------

pub async fn list_sessions(
    State(state): State<AppState>,
    AuthUser(owner): AuthUser,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let sessions = {
        let conn = state.user_db.lock().unwrap_or_else(|p| p.into_inner());
        let rows = db::get_copytrade_sessions(&conn, &owner)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        rows.iter()
            .map(|r| {
                let pv = db::get_session_positions_value(&conn, &r.id).unwrap_or(0.0);
                session_from_row(r, pv)
            })
            .collect::<Vec<CopyTradeSession>>()
    };
    Ok(Json(sessions))
}

// ---------------------------------------------------------------------------
// GET /api/copytrade/sessions/:id
// ---------------------------------------------------------------------------

pub async fn get_session(
    State(state): State<AppState>,
    AuthUser(owner): AuthUser,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let conn = state.user_db.lock().unwrap_or_else(|p| p.into_inner());
    let row = db::get_copytrade_session(&conn, &id, &owner)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    match row {
        Some(r) => {
            let pv = db::get_session_positions_value(&conn, &r.id).unwrap_or(0.0);
            Ok(Json(session_from_row(&r, pv)))
        }
        None => Err((StatusCode::NOT_FOUND, "Session not found".into())),
    }
}

// ---------------------------------------------------------------------------
// PATCH /api/copytrade/sessions/:id
// ---------------------------------------------------------------------------

pub async fn update_session(
    State(state): State<AppState>,
    AuthUser(owner): AuthUser,
    Path(id): Path<String>,
    Json(req): Json<SessionPatchRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Load session to verify ownership
    let row = {
        let conn = state.user_db.lock().unwrap_or_else(|p| p.into_inner());
        db::get_copytrade_session(&conn, &id, &owner)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    };
    let row = row.ok_or((StatusCode::NOT_FOUND, "Session not found".into()))?;
    let current = SessionStatus::from_str(&row.status).ok_or((
        StatusCode::INTERNAL_SERVER_ERROR,
        "Invalid session status".into(),
    ))?;

    let (new_status, cmd) = match req.action.as_str() {
        "pause" => {
            if current != SessionStatus::Running {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "Can only pause a running session".into(),
                ));
            }
            (
                "paused",
                CopyTradeCommand::Pause {
                    session_id: id.clone(),
                },
            )
        }
        "resume" => {
            if current != SessionStatus::Paused {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "Can only resume a paused session".into(),
                ));
            }
            (
                "running",
                CopyTradeCommand::Resume {
                    session_id: id.clone(),
                },
            )
        }
        "stop" => {
            if current == SessionStatus::Stopped {
                return Err((StatusCode::BAD_REQUEST, "Session already stopped".into()));
            }
            (
                "stopped",
                CopyTradeCommand::Stop {
                    session_id: id.clone(),
                },
            )
        }
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                "action must be pause, resume, or stop".into(),
            ));
        }
    };

    // Update DB immediately
    {
        let conn = state.user_db.lock().unwrap_or_else(|p| p.into_inner());
        db::update_session_status(&conn, &id, new_status)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    // Send command to engine
    let _ = state.copytrade_cmd_tx.send(cmd).await;

    // Return updated session
    let conn = state.user_db.lock().unwrap_or_else(|p| p.into_inner());
    let updated = db::get_copytrade_session(&conn, &id, &owner)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    match updated {
        Some(r) => {
            let pv = db::get_session_positions_value(&conn, &r.id).unwrap_or(0.0);
            Ok(Json(session_from_row(&r, pv)))
        }
        None => Err((StatusCode::NOT_FOUND, "Session not found".into())),
    }
}

// ---------------------------------------------------------------------------
// GET /api/copytrade/sessions/:id/orders
// ---------------------------------------------------------------------------

pub async fn list_session_orders(
    State(state): State<AppState>,
    AuthUser(owner): AuthUser,
    Path(id): Path<String>,
    Query(params): Query<SessionOrdersParams>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Verify session ownership
    {
        let conn = state.user_db.lock().unwrap_or_else(|p| p.into_inner());
        let row = db::get_copytrade_session(&conn, &id, &owner)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        if row.is_none() {
            return Err((StatusCode::NOT_FOUND, "Session not found".into()));
        }
    }

    let limit = params.limit.unwrap_or(50).min(200);
    let offset = params.offset.unwrap_or(0);

    let rows = {
        let conn = state.user_db.lock().unwrap_or_else(|p| p.into_inner());
        db::get_session_orders(&conn, &id, limit, offset)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    };

    let orders: Vec<CopyTradeOrder> = rows.into_iter().map(order_from_row).collect();
    Ok(Json(orders))
}

// ---------------------------------------------------------------------------
// DELETE /api/copytrade/sessions/:id
// ---------------------------------------------------------------------------

pub async fn delete_session(
    State(state): State<AppState>,
    AuthUser(owner): AuthUser,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Verify stopped
    let row = {
        let conn = state.user_db.lock().unwrap_or_else(|p| p.into_inner());
        db::get_copytrade_session(&conn, &id, &owner)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    };
    let row = row.ok_or((StatusCode::NOT_FOUND, "Session not found".into()))?;
    if row.status != "stopped" {
        return Err((
            StatusCode::CONFLICT,
            "Session must be stopped before deletion".into(),
        ));
    }

    let deleted = {
        let conn = state.user_db.lock().unwrap_or_else(|p| p.into_inner());
        db::delete_copytrade_session(&conn, &id, &owner)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    };
    if !deleted {
        return Err((StatusCode::NOT_FOUND, "Session not found".into()));
    }

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// POST /api/copytrade/close-position
// ---------------------------------------------------------------------------

pub async fn close_position(
    State(state): State<AppState>,
    AuthUser(owner): AuthUser,
    Json(req): Json<ClosePositionRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    use polymarket_client_sdk::clob::types::{Amount, OrderType, Side};
    use rust_decimal::Decimal;
    use std::str::FromStr;

    // Verify session ownership
    let session_row = {
        let conn = state.user_db.lock().unwrap_or_else(|p| p.into_inner());
        db::get_copytrade_session(&conn, &req.session_id, &owner)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    };
    let session_row = session_row.ok_or((StatusCode::NOT_FOUND, "Session not found".into()))?;

    // Compute net shares
    let net_shares = {
        let conn = state.user_db.lock().unwrap_or_else(|p| p.into_inner());
        db::get_net_shares(&conn, &req.session_id, &req.asset_id)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    };

    if net_shares <= 0.0 {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("No shares to close (net: {net_shares:.2})"),
        ));
    }

    // For simulation sessions, simulate the close
    if session_row.simulate {
        let order_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        // Use last fill price from DB as best available price estimate
        let last_fill = {
            let conn = state.user_db.lock().unwrap_or_else(|p| p.into_inner());
            db::get_last_fill_price(&conn, &req.session_id, &req.asset_id)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        };
        let fill_price = match last_fill {
            Some(p) if p > 0.0 => p,
            _ => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "No fill price available for this asset. Cannot close position.".into(),
                ));
            }
        };

        let size_usdc = net_shares * fill_price;

        let order_row = db::CopyTradeOrderRow {
            id: order_id.clone(),
            session_id: req.session_id.clone(),
            source_tx_hash: "close-position".to_string(),
            source_trader: owner.clone(),
            clob_order_id: None,
            asset_id: req.asset_id.clone(),
            side: "sell".to_string(),
            price: fill_price,
            source_price: fill_price,
            size_usdc,
            size_shares: Some(net_shares),
            status: "simulated".to_string(),
            error_message: None,
            fill_price: Some(fill_price),
            slippage_bps: Some(0.0),
            tx_hash: None,
            created_at: now.clone(),
            updated_at: now,
        };

        {
            let conn = state.user_db.lock().unwrap_or_else(|p| p.into_inner());
            db::insert_copytrade_order(&conn, &order_row)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            // Update remaining_capital: add sale proceeds
            let new_capital = session_row.remaining_capital + size_usdc;
            db::update_session_capital(&conn, &req.session_id, new_capital)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        }

        let _ = state
            .copytrade_update_tx
            .send(CopyTradeUpdate::OrderPlaced {
                session_id: req.session_id.clone(),
                order: CopyTradeOrderSummary {
                    id: order_id.clone(),
                    asset_id: req.asset_id.clone(),
                    side: "sell".to_string(),
                    size_usdc,
                    price: fill_price,
                    source_trader: owner,
                    simulate: true,
                },
                owner: session_row.owner.clone(),
            });

        return Ok(Json(serde_json::json!({
            "order_id": order_id,
            "status": "simulated",
            "shares_sold": net_shares,
            "estimated_usdc": size_usdc,
        })));
    }

    // Live close: place FOK sell via CLOB
    let clob = state.clob_client.read().await;
    let cs = clob.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "CLOB client not initialized".into(),
    ))?;

    let token_id = polymarket_client_sdk::types::U256::from_str(&req.asset_id)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid asset_id: {e}")))?;

    let shares_dec = Decimal::from_f64_retain(net_shares)
        .unwrap_or(Decimal::ZERO)
        .trunc_with_scale(2);
    let amount = Amount::shares(shares_dec).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("Invalid shares amount: {e}"),
        )
    })?;

    let signable = cs
        .client
        .market_order()
        .token_id(token_id)
        .side(Side::Sell)
        .amount(amount)
        .order_type(OrderType::FOK)
        .build()
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Order build failed: {e}"),
            )
        })?;

    let signed = cs.client.sign(&cs.signer, signable).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Sign failed: {e}"),
        )
    })?;

    let resp = cs
        .client
        .post_order(signed)
        .await
        .map_err(|e| (StatusCode::SERVICE_UNAVAILABLE, format!("CLOB error: {e}")))?;

    drop(clob);

    // Record order
    let order_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let status = if resp.success
        && resp.status == polymarket_client_sdk::clob::types::OrderStatusType::Matched
    {
        "filled"
    } else {
        "failed"
    };

    use rust_decimal::prelude::ToPrimitive;
    // Sell: taking=USDC received, making=shares sent → price = taking/making
    let fill_price = if resp.taking_amount > Decimal::ZERO && resp.making_amount > Decimal::ZERO {
        resp.taking_amount.to_f64().unwrap_or(0.0) / resp.making_amount.to_f64().unwrap_or(1.0)
    } else {
        0.0
    };
    let actual_usdc = resp.taking_amount.to_f64().unwrap_or(0.0);

    let order_row = db::CopyTradeOrderRow {
        id: order_id.clone(),
        session_id: req.session_id.clone(),
        source_tx_hash: "close-position".to_string(),
        source_trader: owner.clone(),
        clob_order_id: Some(resp.order_id.clone()),
        asset_id: req.asset_id.clone(),
        side: "sell".to_string(),
        price: fill_price,
        source_price: fill_price,
        size_usdc: actual_usdc,
        size_shares: Some(net_shares),
        status: status.to_string(),
        error_message: resp.error_msg.clone(),
        fill_price: if status == "filled" {
            Some(fill_price)
        } else {
            None
        },
        slippage_bps: None,
        tx_hash: resp.transaction_hashes.first().map(|h| h.to_string()),
        created_at: now.clone(),
        updated_at: now,
    };

    {
        let conn = state.user_db.lock().unwrap_or_else(|p| p.into_inner());
        let _ = db::insert_copytrade_order(&conn, &order_row);
    }

    Ok(Json(serde_json::json!({
        "order_id": order_id,
        "clob_order_id": resp.order_id,
        "status": status,
        "shares_sold": net_shares,
        "success": resp.success,
    })))
}

// ---------------------------------------------------------------------------
// GET /api/copytrade/sessions/:id/stats
// ---------------------------------------------------------------------------

pub async fn get_session_stats(
    State(state): State<AppState>,
    AuthUser(owner): AuthUser,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let (session_row, order_stats, positions) = {
        let conn = state.user_db.lock().unwrap_or_else(|p| p.into_inner());
        let row = db::get_copytrade_session(&conn, &id, &owner)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or((StatusCode::NOT_FOUND, "Session not found".into()))?;
        let stats = db::get_session_order_stats(&conn, &id)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let positions = db::get_positions_raw(&conn, &id)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        (row, stats, positions)
    };

    // Fetch live CLOB prices for all position assets
    let asset_ids: Vec<String> = positions.iter().map(|p| p.asset_id.clone()).collect();
    let clob_prices = fetch_clob_midpoints(&state.http, &asset_ids).await;

    // Compute per-asset P&L and win/loss using live prices
    let mut unrealized_pnl = 0.0;
    let mut realized_pnl = 0.0;
    let mut win_count: u32 = 0;
    let mut loss_count: u32 = 0;

    for pos in &positions {
        let cost_per_share = if pos.buy_shares > 0.0 {
            pos.cost_basis / pos.buy_shares
        } else {
            0.0
        };
        let pos_realized = pos.sell_proceeds - (pos.sell_shares * cost_per_share);
        realized_pnl += pos_realized;

        // Use live CLOB price when available, fall back to last fill price
        let live_price = clob_prices
            .get(&pos.asset_id)
            .copied()
            .unwrap_or(pos.last_fill_price);

        if pos.net_shares > 0.001 {
            let remaining_cost = pos.net_shares * cost_per_share;
            let current_value = pos.net_shares * live_price;
            unrealized_pnl += current_value - remaining_cost;
        }

        let pos_unrealized = if pos.net_shares > 0.001 {
            pos.net_shares * live_price - pos.net_shares * cost_per_share
        } else {
            0.0
        };
        if pos_realized + pos_unrealized > 0.0 {
            win_count += 1;
        } else if pos_realized + pos_unrealized < 0.0 {
            loss_count += 1;
        }
    }

    let total_pnl = realized_pnl + unrealized_pnl;
    let return_pct = if session_row.initial_capital > 0.0 {
        total_pnl / session_row.initial_capital * 100.0
    } else {
        0.0
    };
    let win_total = win_count + loss_count;
    let win_rate = if win_total > 0 {
        (win_count as f64 / win_total as f64) * 100.0
    } else {
        0.0
    };

    let capital_utilization = if session_row.initial_capital > 0.0 {
        (session_row.initial_capital - session_row.remaining_capital) / session_row.initial_capital
    } else {
        0.0
    };

    let runtime_seconds = chrono::DateTime::parse_from_rfc3339(&session_row.created_at)
        .map(|created| (chrono::Utc::now() - created.with_timezone(&chrono::Utc)).num_seconds())
        .unwrap_or(0);

    Ok(Json(SessionStats {
        total_orders: order_stats.total_orders,
        filled_orders: order_stats.filled_orders,
        failed_orders: order_stats.failed_orders,
        pending_orders: order_stats.pending_orders,
        canceled_orders: order_stats.canceled_orders,
        total_invested: order_stats.total_invested,
        total_returned: order_stats.total_returned,
        realized_pnl,
        unrealized_pnl,
        total_pnl,
        return_pct,
        win_count,
        loss_count,
        win_rate,
        avg_slippage_bps: order_stats.avg_slippage_bps,
        max_slippage_bps: order_stats.max_slippage_bps,
        capital_utilization,
        runtime_seconds,
    }))
}

// ---------------------------------------------------------------------------
// GET /api/copytrade/sessions/:id/positions
// ---------------------------------------------------------------------------

pub async fn get_session_positions(
    State(state): State<AppState>,
    AuthUser(owner): AuthUser,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let positions = {
        let conn = state.user_db.lock().unwrap_or_else(|p| p.into_inner());
        let _row = db::get_copytrade_session(&conn, &id, &owner)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or((StatusCode::NOT_FOUND, "Session not found".into()))?;
        db::get_positions_raw(&conn, &id)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    };

    // Enrich with market metadata + live CLOB prices
    let asset_ids: Vec<String> = positions.iter().map(|p| p.asset_id.clone()).collect();
    let (market_info, clob_prices) = tokio::join!(
        super::markets::resolve_markets(&state.http, &state.db, &state.market_cache, &asset_ids),
        fetch_clob_midpoints(&state.http, &asset_ids),
    );

    let result: Vec<CopyTradePosition> = positions
        .into_iter()
        .map(|p| {
            let info = market_info.get(&p.asset_id);
            let cost_per_share = if p.buy_shares > 0.0 {
                p.cost_basis / p.buy_shares
            } else {
                0.0
            };
            // Use live CLOB price when available, fall back to last fill price
            let live_price = clob_prices
                .get(&p.asset_id)
                .copied()
                .unwrap_or(p.last_fill_price);
            let current_value = p.net_shares * live_price;
            let remaining_cost = p.net_shares * cost_per_share;
            let pos_realized = p.sell_proceeds - (p.sell_shares * cost_per_share);

            CopyTradePosition {
                asset_id: p.asset_id,
                question: info.map(|i| i.question.clone()).unwrap_or_default(),
                outcome: info.map(|i| i.outcome.clone()).unwrap_or_default(),
                category: info.map(|i| i.category.clone()).unwrap_or_default(),
                buy_shares: p.buy_shares,
                sell_shares: p.sell_shares,
                net_shares: p.net_shares,
                avg_entry_price: cost_per_share,
                current_price: live_price,
                last_fill_price: p.last_fill_price,
                cost_basis: p.cost_basis,
                current_value,
                unrealized_pnl: current_value - remaining_cost,
                realized_pnl: pos_realized,
                order_count: p.order_count,
                source_traders: p
                    .source_traders
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .collect(),
                last_order_at: p.last_order_at,
            }
        })
        .collect();

    Ok(Json(result))
}

// ---------------------------------------------------------------------------
// GET /api/copytrade/summary
// ---------------------------------------------------------------------------

pub async fn get_summary(
    State(state): State<AppState>,
    AuthUser(owner): AuthUser,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Single lock acquisition: load sessions, order count, and all positions at once
    let (active_sessions, total_orders, all_positions) = {
        let conn = state.user_db.lock().unwrap_or_else(|p| p.into_inner());
        let sessions = db::get_copytrade_sessions(&conn, &owner)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let total_orders = db::get_total_order_count(&conn, &owner)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let active = sessions
            .iter()
            .filter(|s| s.status == "running" || s.status == "paused")
            .count() as u32;
        let positions: Vec<(f64, Vec<db::PositionRaw>)> = sessions
            .iter()
            .map(|s| {
                let pos = db::get_positions_raw(&conn, &s.id).unwrap_or_default();
                (s.initial_capital, pos)
            })
            .collect();
        (active, total_orders, positions)
    };

    // Collect all unique asset IDs for a single batch CLOB fetch
    let all_asset_ids: Vec<String> = all_positions
        .iter()
        .flat_map(|(_, positions)| positions.iter().map(|p| p.asset_id.clone()))
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    let clob_prices = fetch_clob_midpoints(&state.http, &all_asset_ids).await;

    // Compute total P&L across all sessions using live CLOB prices
    let mut total_pnl = 0.0;
    let mut total_initial = 0.0;
    for (initial_capital, positions) in &all_positions {
        let mut session_pnl = 0.0;
        for pos in positions {
            let cost_per_share = if pos.buy_shares > 0.0 {
                pos.cost_basis / pos.buy_shares
            } else {
                0.0
            };
            let pos_realized = pos.sell_proceeds - (pos.sell_shares * cost_per_share);
            session_pnl += pos_realized;

            let live_price = clob_prices
                .get(&pos.asset_id)
                .copied()
                .unwrap_or(pos.last_fill_price);
            if pos.net_shares > 0.001 {
                let remaining_cost = pos.net_shares * cost_per_share;
                let current_value = pos.net_shares * live_price;
                session_pnl += current_value - remaining_cost;
            }
        }
        total_pnl += session_pnl;
        total_initial += initial_capital;
    }
    let total_return_pct = if total_initial > 0.0 {
        total_pnl / total_initial * 100.0
    } else {
        0.0
    };

    Ok(Json(CopyTradeSummary {
        active_sessions,
        total_pnl,
        total_return_pct,
        total_orders,
    }))
}

// ---------------------------------------------------------------------------
// GET /api/copytrade/active-traders
// Returns the set of source trader addresses across all active sessions.
// Used by frontend to set up filtered WS subscriptions.
// ---------------------------------------------------------------------------

pub async fn get_active_traders(
    State(state): State<AppState>,
    AuthUser(owner): AuthUser,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let sessions = {
        let conn = state.user_db.lock().unwrap_or_else(|p| p.into_inner());
        db::get_copytrade_sessions(&conn, &owner)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    };

    let active_sessions: Vec<_> = sessions
        .into_iter()
        .filter(|s| s.status == "running" || s.status == "paused")
        .collect();

    let mut all_traders = std::collections::HashSet::new();
    for session in &active_sessions {
        match super::engine::resolve_session_traders(&state.user_db, &state.db, session).await {
            Ok(traders) => all_traders.extend(traders),
            Err(e) => tracing::warn!("Failed to resolve traders for session {}: {e}", session.id),
        }
    }

    let traders: Vec<String> = all_traders.into_iter().collect();
    Ok(Json(traders))
}

// ---------------------------------------------------------------------------
// Public CLOB price fetch (no auth required)
// ---------------------------------------------------------------------------

async fn fetch_clob_midpoints(
    http: &reqwest::Client,
    token_ids: &[String],
) -> std::collections::HashMap<String, f64> {
    let mut handles = Vec::with_capacity(token_ids.len());
    for tid in token_ids {
        let http = http.clone();
        let tid = tid.clone();
        handles.push(tokio::spawn(async move {
            let buy = fetch_one_price(&http, &tid, "BUY").await;
            let sell = fetch_one_price(&http, &tid, "SELL").await;
            let mid = match (buy, sell) {
                (Some(b), Some(s)) => (b + s) / 2.0,
                (Some(b), None) => b,
                (None, Some(s)) => s,
                (None, None) => return None,
            };
            Some((tid, mid))
        }));
    }

    let mut result = std::collections::HashMap::new();
    for handle in handles {
        if let Ok(Some((tid, price))) = handle.await {
            result.insert(tid, price);
        }
    }
    result
}

async fn fetch_one_price(http: &reqwest::Client, token_id: &str, side: &str) -> Option<f64> {
    #[derive(serde::Deserialize)]
    struct PriceResp {
        price: Option<String>,
    }
    let url = format!(
        "https://clob.polymarket.com/price?token_id={}&side={}",
        token_id, side
    );
    let resp = http
        .get(&url)
        .timeout(std::time::Duration::from_secs(3))
        .send()
        .await
        .ok()?;
    let body: PriceResp = resp.json().await.ok()?;
    body.price?.parse::<f64>().ok()
}

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

fn session_from_row(row: &CopyTradeSessionRow, positions_value: f64) -> CopyTradeSession {
    CopyTradeSession {
        id: row.id.clone(),
        list_id: row.list_id.clone(),
        top_n: row.top_n,
        copy_pct: row.copy_pct,
        max_position_usdc: row.max_position_usdc,
        max_slippage_bps: row.max_slippage_bps,
        order_type: CopyOrderType::from_str(&row.order_type).unwrap_or(CopyOrderType::FOK),
        initial_capital: row.initial_capital,
        remaining_capital: row.remaining_capital,
        positions_value,
        simulate: row.simulate,
        max_loss_pct: row.max_loss_pct,
        status: SessionStatus::from_str(&row.status).unwrap_or(SessionStatus::Stopped),
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
    }
}

fn order_from_row(row: db::CopyTradeOrderRow) -> CopyTradeOrder {
    CopyTradeOrder {
        id: row.id,
        session_id: row.session_id,
        source_tx_hash: row.source_tx_hash,
        source_trader: row.source_trader,
        clob_order_id: row.clob_order_id,
        asset_id: row.asset_id,
        side: row.side,
        price: row.price,
        source_price: row.source_price,
        size_usdc: row.size_usdc,
        size_shares: row.size_shares,
        status: OrderStatus::from_str(&row.status).unwrap_or(OrderStatus::Failed),
        error_message: row.error_message,
        fill_price: row.fill_price,
        slippage_bps: row.slippage_bps,
        tx_hash: row.tx_hash,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}
