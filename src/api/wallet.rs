use alloy::primitives::{Address, U256};
use alloy::providers::Provider;
use alloy::signers::Signer as _;
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
};
use secrecy::ExposeSecret;
use std::str::FromStr;

use super::contracts;
use super::db::{self, WalletError};
use super::middleware::AuthUser;
use super::server::AppState;
use super::types::{
    ApprovalResult, DepositAddresses, DepositStatus, DeriveCredentialsResponse,
    ImportWalletRequest, ImportWalletResponse, PendingDeposit, TradingWalletInfo, WalletBalance,
    WalletGenerateResponse,
};

/// Derives proxy wallet address using the SDK's official CREATE2 computation.
fn proxy_address_for(eoa: &[u8; 20]) -> String {
    let addr = alloy_primitives::Address::from_slice(eoa);
    match polymarket_client_sdk::derive_proxy_wallet(addr, polymarket_client_sdk::POLYGON) {
        Some(proxy) => format!("0x{}", hex::encode(proxy.as_slice())),
        None => String::new(),
    }
}

/// Derives an Ethereum address from a secp256k1 signing key.
fn address_from_signing_key(signing_key: &k256::ecdsa::SigningKey) -> [u8; 20] {
    let verify_key = signing_key.verifying_key();
    let public_key_bytes = verify_key.to_encoded_point(false);
    let pub_hash = alloy_primitives::keccak256(&public_key_bytes.as_bytes()[1..]);
    let mut address = [0u8; 20];
    address.copy_from_slice(&pub_hash[12..]);
    address
}

fn format_address(bytes: &[u8; 20]) -> String {
    format!("0x{}", hex::encode(bytes))
}

fn map_wallet_error(e: WalletError) -> (StatusCode, String) {
    match e {
        WalletError::LimitReached => (
            StatusCode::CONFLICT,
            format!("Wallet limit reached (max {}).", db::MAX_WALLETS_PER_USER),
        ),
        WalletError::NotFound => (StatusCode::NOT_FOUND, "No trading wallet found".into()),
        WalletError::Db(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

// ---------------------------------------------------------------------------
// GET /api/wallets
// ---------------------------------------------------------------------------

pub async fn get_wallets(
    State(state): State<AppState>,
    AuthUser(owner): AuthUser,
) -> Result<Json<Vec<TradingWalletInfo>>, (StatusCode, String)> {
    let owner = owner.to_lowercase();
    let rows = tokio::task::spawn_blocking({
        let state = state.clone();
        let owner = owner.clone();
        move || {
            let conn = state.user_db.lock().expect("user_db lock");
            db::get_trading_wallets(&conn, &owner)
        }
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let wallets = rows
        .into_iter()
        .map(|w| TradingWalletInfo {
            id: w.id,
            address: w.wallet_address,
            proxy_address: w.proxy_address,
            status: w.status,
            has_clob_credentials: w.clob_api_key.is_some(),
            created_at: w.created_at,
        })
        .collect();

    Ok(Json(wallets))
}

// ---------------------------------------------------------------------------
// POST /api/wallets/generate
// ---------------------------------------------------------------------------

pub async fn generate_wallet(
    State(state): State<AppState>,
    AuthUser(owner): AuthUser,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let owner = owner.to_lowercase();

    // Generate a random secp256k1 signing key
    let signing_key = k256::ecdsa::SigningKey::random(&mut k256::elliptic_curve::rand_core::OsRng);
    let private_key_bytes = signing_key.to_bytes();
    let address = address_from_signing_key(&signing_key);

    // Derive proxy wallet using SDK's official CREATE2 computation
    let proxy_addr = proxy_address_for(&address);

    // Encrypt the private key
    let encryption_key = super::crypto::derive_user_key(&state.encryption_key, &owner);
    let (encrypted_key, key_nonce) =
        super::crypto::encrypt_secret(&encryption_key, &private_key_bytes, owner.as_bytes())
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let wallet_addr = format_address(&address);
    let private_key_hex = format!("0x{}", hex::encode(&private_key_bytes));

    // Store in SQLite (count check happens inside create_trading_wallet)
    let wallet_id = tokio::task::spawn_blocking({
        let state = state.clone();
        let owner = owner.clone();
        let wallet_addr = wallet_addr.clone();
        let proxy_addr = proxy_addr.clone();
        move || {
            let conn = state.user_db.lock().expect("user_db lock");
            db::create_trading_wallet(
                &conn,
                &owner,
                &wallet_addr,
                &proxy_addr,
                &encrypted_key,
                &key_nonce,
            )
        }
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(map_wallet_error)?;

    // Build response with no-cache headers
    let mut headers = HeaderMap::new();
    headers.insert("Cache-Control", HeaderValue::from_static("no-store"));
    headers.insert("Pragma", HeaderValue::from_static("no-cache"));

    Ok((
        headers,
        Json(WalletGenerateResponse {
            id: wallet_id,
            address: wallet_addr,
            private_key: private_key_hex,
            proxy_address: proxy_addr,
        }),
    ))
}

// ---------------------------------------------------------------------------
// POST /api/wallets/import
// ---------------------------------------------------------------------------

pub async fn import_wallet(
    State(state): State<AppState>,
    AuthUser(owner): AuthUser,
    Json(body): Json<ImportWalletRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let owner = owner.to_lowercase();

    // Validate and parse private key
    let key_hex = body
        .private_key
        .strip_prefix("0x")
        .unwrap_or(&body.private_key);

    if key_hex.len() != 64 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Invalid private key format. Expected 0x + 64 hex characters.".into(),
        ));
    }

    let key_bytes = hex::decode(key_hex).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            "Invalid private key format. Expected 0x + 64 hex characters.".into(),
        )
    })?;

    // Derive address from private key
    let signing_key =
        k256::ecdsa::SigningKey::from_bytes(key_bytes.as_slice().into()).map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                "Invalid private key. Could not derive signing key.".into(),
            )
        })?;

    let address = address_from_signing_key(&signing_key);
    let proxy_addr = proxy_address_for(&address);

    // Encrypt the private key
    let encryption_key = super::crypto::derive_user_key(&state.encryption_key, &owner);
    let (encrypted_key, key_nonce) =
        super::crypto::encrypt_secret(&encryption_key, &key_bytes, owner.as_bytes())
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let wallet_addr = format_address(&address);

    // Store in SQLite (count check happens inside create_trading_wallet)
    let wallet_id = tokio::task::spawn_blocking({
        let state = state.clone();
        let owner = owner.clone();
        let wallet_addr = wallet_addr.clone();
        let proxy_addr = proxy_addr.clone();
        move || {
            let conn = state.user_db.lock().expect("user_db lock");
            db::create_trading_wallet(
                &conn,
                &owner,
                &wallet_addr,
                &proxy_addr,
                &encrypted_key,
                &key_nonce,
            )
        }
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(map_wallet_error)?;

    Ok(Json(ImportWalletResponse {
        id: wallet_id,
        address: wallet_addr,
        proxy_address: proxy_addr,
    }))
}

// ---------------------------------------------------------------------------
// POST /api/wallets/:id/derive-credentials
// ---------------------------------------------------------------------------

pub async fn derive_credentials(
    State(state): State<AppState>,
    AuthUser(owner): AuthUser,
    Path(wallet_id): Path<String>,
) -> Result<Json<DeriveCredentialsResponse>, (StatusCode, String)> {
    let owner = owner.to_lowercase();

    // 1. Load wallet from SQLite (by owner + id for ownership check)
    let row = tokio::task::spawn_blocking({
        let state = state.clone();
        let owner = owner.clone();
        let wallet_id = wallet_id.clone();
        move || {
            let conn = state.user_db.lock().expect("user_db lock");
            db::get_trading_wallet_by_id(&conn, &owner, &wallet_id)
        }
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or_else(|| (StatusCode::NOT_FOUND, "No trading wallet found".into()))?;

    // 2. Decrypt private key
    let encryption_key = super::crypto::derive_user_key(&state.encryption_key, &owner);
    let private_key_bytes = super::crypto::decrypt_secret(
        &encryption_key,
        &row.encrypted_key,
        &row.key_nonce,
        owner.as_bytes(),
    )
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Decryption failed: {e}"),
        )
    })?;

    let private_key_hex = format!("0x{}", hex::encode(&private_key_bytes));

    // 3. Create signer and derive CLOB credentials via SDK
    let signer = alloy::signers::local::LocalSigner::from_str(&private_key_hex)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Signer creation failed: {e}"),
            )
        })?
        .with_chain_id(Some(polymarket_client_sdk::POLYGON));

    let config = polymarket_client_sdk::clob::Config::builder()
        .use_server_time(true)
        .build();

    let clob_client =
        polymarket_client_sdk::clob::Client::new("https://clob.polymarket.com", config).map_err(
            |e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("CLOB client error: {e}"),
                )
            },
        )?;

    let credentials = clob_client
        .create_or_derive_api_key(&signer, None)
        .await
        .map_err(|e| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                format!("CLOB API error: {e}"),
            )
        })?;

    // 4. Encrypt credentials (secret + passphrase as JSON blob)
    let api_key = credentials.key().to_string();
    let cred_json = serde_json::json!({
        "secret": credentials.secret().expose_secret(),
        "passphrase": credentials.passphrase().expose_secret(),
    });
    let cred_bytes = serde_json::to_vec(&cred_json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let (cred_blob, cred_nonce) =
        super::crypto::encrypt_secret(&encryption_key, &cred_bytes, owner.as_bytes())
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    // 5. Store encrypted credentials in SQLite
    tokio::task::spawn_blocking({
        let state = state.clone();
        let owner = owner.clone();
        let wallet_id = wallet_id.clone();
        let api_key = api_key.clone();
        move || {
            let conn = state.user_db.lock().expect("user_db lock");
            db::update_wallet_credentials(
                &conn,
                &owner,
                &wallet_id,
                &api_key,
                &cred_blob,
                &cred_nonce,
            )
        }
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(map_wallet_error)?;

    Ok(Json(DeriveCredentialsResponse {
        success: true,
        wallet_id,
        api_key,
    }))
}

// ---------------------------------------------------------------------------
// DELETE /api/wallets/:id
// ---------------------------------------------------------------------------

pub async fn delete_wallet(
    State(state): State<AppState>,
    AuthUser(owner): AuthUser,
    Path(wallet_id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let owner = owner.to_lowercase();

    // Block deletion if wallet is backing an active copy-trade session
    {
        let conn = state.user_db.lock().unwrap_or_else(|p| p.into_inner());
        let has_active = db::has_active_copytrade_session(&conn, &owner)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        if has_active {
            return Err((
                StatusCode::CONFLICT,
                "Cannot delete wallet while a copy-trade session is active. Stop the session first.".into(),
            ));
        }
    }

    tokio::task::spawn_blocking({
        let state = state.clone();
        let owner = owner.clone();
        move || {
            let conn = state.user_db.lock().expect("user_db lock");
            db::delete_trading_wallet(&conn, &owner, &wallet_id)
        }
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(map_wallet_error)?;

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// GET /api/wallets/:id/balance
// ---------------------------------------------------------------------------

pub async fn get_balance(
    State(state): State<AppState>,
    AuthUser(owner): AuthUser,
    Path(wallet_id): Path<String>,
) -> Result<Json<WalletBalance>, (StatusCode, String)> {
    let owner = owner.to_lowercase();

    // Verify wallet ownership
    let row = load_wallet(&state, &owner, &wallet_id).await?;

    // Try cache first (balance poll updates every 30s)
    let cached = state.wallet_balances.read().await.get(&wallet_id).cloned();

    if let Some(entry) = cached {
        let secs_ago = entry.last_checked.elapsed().as_secs();
        let pol_wei: U256 = entry.pol_raw.parse().unwrap_or_default();
        let pol_low = pol_wei < contracts::MIN_POL_WEI;
        return Ok(Json(WalletBalance {
            usdc_balance: entry.usdc_balance,
            usdc_raw: entry.usdc_raw,
            ctf_exchange_approved: entry.ctf_approved,
            neg_risk_exchange_approved: entry.neg_risk_approved,
            pol_balance: entry.pol_balance,
            needs_gas: pol_low,
            last_checked_secs_ago: Some(secs_ago),
        }));
    }

    // Cache miss — do a live RPC query
    // USDC.e balance on proxy; allowances + POL on EOA
    let eoa: Address = row.wallet_address.parse().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Invalid wallet address in DB".into(),
        )
    })?;
    let proxy = row
        .proxy_address
        .as_deref()
        .and_then(|s| s.parse::<Address>().ok())
        .unwrap_or(eoa);

    let provider = contracts::create_provider(&state.erpc_url);
    let usdc = contracts::IERC20::new(contracts::USDC_ADDRESS, &provider);

    let bal_call = usdc.balanceOf(proxy);
    let ctf_call = usdc.allowance(eoa, contracts::CTF_EXCHANGE);
    let neg_call = usdc.allowance(eoa, contracts::NEG_RISK_EXCHANGE);
    let (balance_res, ctf_res, neg_res, pol_res) = tokio::join!(
        bal_call.call(),
        ctf_call.call(),
        neg_call.call(),
        provider.get_balance(eoa),
    );

    let usdc_raw = balance_res.map_err(|e| (StatusCode::BAD_GATEWAY, format!("RPC error: {e}")))?;
    let ctf_allowance = ctf_res.unwrap_or_default();
    let neg_allowance = neg_res.unwrap_or_default();
    let pol_wei = pol_res.map_err(|e| (StatusCode::BAD_GATEWAY, format!("RPC error: {e}")))?;

    // Update cache
    let entry = super::server::WalletBalanceState {
        usdc_balance: contracts::format_usdc(usdc_raw),
        usdc_raw: usdc_raw.to_string(),
        pol_balance: contracts::format_pol(pol_wei),
        pol_raw: pol_wei.to_string(),
        ctf_approved: !ctf_allowance.is_zero(),
        neg_risk_approved: !neg_allowance.is_zero(),
        last_checked: std::time::Instant::now(),
    };
    state
        .wallet_balances
        .write()
        .await
        .insert(wallet_id, entry.clone());

    Ok(Json(WalletBalance {
        usdc_balance: entry.usdc_balance,
        usdc_raw: entry.usdc_raw,
        ctf_exchange_approved: entry.ctf_approved,
        neg_risk_exchange_approved: entry.neg_risk_approved,
        pol_balance: entry.pol_balance,
        needs_gas: pol_wei < contracts::MIN_POL_WEI,
        last_checked_secs_ago: Some(0),
    }))
}

// ---------------------------------------------------------------------------
// POST /api/wallets/:id/approve
// ---------------------------------------------------------------------------

pub async fn approve_exchanges(
    State(state): State<AppState>,
    AuthUser(owner): AuthUser,
    Path(wallet_id): Path<String>,
) -> Result<Json<ApprovalResult>, (StatusCode, String)> {
    let owner = owner.to_lowercase();
    let row = load_wallet(&state, &owner, &wallet_id).await?;

    let eoa: Address = row.wallet_address.parse().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Invalid wallet address in DB".into(),
        )
    })?;

    // Check POL balance on EOA (gas payer)
    let provider = contracts::create_provider(&state.erpc_url);
    let pol_wei = provider
        .get_balance(eoa)
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("RPC error: {e}")))?;

    if pol_wei < contracts::MIN_POL_WEI {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Insufficient POL for gas. Send ~0.01 POL to {}. Current: {} POL",
                row.wallet_address,
                contracts::format_pol(pol_wei),
            ),
        ));
    }

    // Check current allowances on EOA (EOA signs approve + exchange pulls from EOA)
    let usdc_read = contracts::IERC20::new(contracts::USDC_ADDRESS, &provider);
    let ctf_call = usdc_read.allowance(eoa, contracts::CTF_EXCHANGE);
    let neg_call = usdc_read.allowance(eoa, contracts::NEG_RISK_EXCHANGE);
    let (ctf_res, neg_res) = tokio::join!(ctf_call.call(), neg_call.call(),);
    let ctf_allowance = ctf_res.unwrap_or_default();
    let neg_allowance = neg_res.unwrap_or_default();

    if !ctf_allowance.is_zero() && !neg_allowance.is_zero() {
        return Ok(Json(ApprovalResult {
            ctf_tx_hash: None,
            neg_risk_tx_hash: None,
            already_approved: true,
        }));
    }

    // Decrypt private key and create signing provider
    let encryption_key = super::crypto::derive_user_key(&state.encryption_key, &owner);
    let private_key_bytes = super::crypto::decrypt_secret(
        &encryption_key,
        &row.encrypted_key,
        &row.key_nonce,
        owner.as_bytes(),
    )
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Decryption failed: {e}"),
        )
    })?;

    let private_key_hex = format!("0x{}", hex::encode(&private_key_bytes));
    let signer = alloy::signers::local::PrivateKeySigner::from_str(&private_key_hex)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Signer error: {e}"),
            )
        })?
        .with_chain_id(Some(137)); // Polygon

    let wallet_provider = contracts::create_wallet_provider(signer, &state.erpc_url);
    let usdc = contracts::IERC20::new(contracts::USDC_ADDRESS, &wallet_provider);

    let mut ctf_tx_hash = None;
    let mut neg_risk_tx_hash = None;

    // Approve CTF Exchange if needed
    if ctf_allowance.is_zero() {
        match usdc
            .approve(contracts::CTF_EXCHANGE, U256::MAX)
            .send()
            .await
        {
            Ok(pending) => match pending.get_receipt().await {
                Ok(receipt) => {
                    ctf_tx_hash = Some(receipt.transaction_hash.to_string());
                }
                Err(e) => {
                    state.wallet_balances.write().await.remove(&wallet_id);
                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("CTF approve receipt failed: {e}"),
                    ));
                }
            },
            Err(e) => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("CTF approve send failed: {e}"),
                ));
            }
        }
    }

    // Approve NegRisk Exchange if needed
    if neg_allowance.is_zero() {
        match usdc
            .approve(contracts::NEG_RISK_EXCHANGE, U256::MAX)
            .send()
            .await
        {
            Ok(pending) => match pending.get_receipt().await {
                Ok(receipt) => {
                    neg_risk_tx_hash = Some(receipt.transaction_hash.to_string());
                }
                Err(e) => {
                    // CTF may have succeeded — invalidate cache so poll picks up partial state
                    state.wallet_balances.write().await.remove(&wallet_id);
                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!(
                            "NegRisk approve failed (CTF may have succeeded: {:?}): {e}",
                            ctf_tx_hash
                        ),
                    ));
                }
            },
            Err(e) => {
                state.wallet_balances.write().await.remove(&wallet_id);
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!(
                        "NegRisk approve send failed (CTF may have succeeded: {:?}): {e}",
                        ctf_tx_hash
                    ),
                ));
            }
        }
    }

    // Invalidate balance cache so next poll picks up new allowances
    state.wallet_balances.write().await.remove(&wallet_id);

    Ok(Json(ApprovalResult {
        ctf_tx_hash,
        neg_risk_tx_hash,
        already_approved: false,
    }))
}

// ---------------------------------------------------------------------------
// GET /api/wallets/:id/deposit-address
// ---------------------------------------------------------------------------

pub async fn get_deposit_address(
    State(state): State<AppState>,
    AuthUser(owner): AuthUser,
    Path(wallet_id): Path<String>,
) -> Result<Json<DepositAddresses>, (StatusCode, String)> {
    let owner = owner.to_lowercase();
    let row = load_wallet(&state, &owner, &wallet_id).await?;

    let proxy_address = row
        .proxy_address
        .unwrap_or_else(|| row.wallet_address.clone());

    // Call Polymarket Bridge API (POST /deposit with JSON body)
    let resp = state
        .http
        .post("https://bridge.polymarket.com/deposit")
        .json(&serde_json::json!({ "address": proxy_address }))
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Bridge API error: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            format!("Bridge API returned {status}: {body}"),
        ));
    }

    let data: serde_json::Value = resp.json().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            format!("Bridge API parse error: {e}"),
        )
    })?;

    // Response has nested "address" object: { address: { evm, svm, btc }, note }
    let addrs = &data["address"];
    Ok(Json(DepositAddresses {
        evm: addrs["evm"].as_str().unwrap_or("").to_string(),
        svm: addrs["svm"].as_str().unwrap_or("").to_string(),
        btc: addrs["btc"].as_str().unwrap_or("").to_string(),
        note: data["note"].as_str().map(String::from),
    }))
}

// ---------------------------------------------------------------------------
// GET /api/wallets/:id/deposit-status
// ---------------------------------------------------------------------------

pub async fn get_deposit_status(
    State(state): State<AppState>,
    AuthUser(owner): AuthUser,
    Path(wallet_id): Path<String>,
) -> Result<Json<DepositStatus>, (StatusCode, String)> {
    let owner = owner.to_lowercase();
    let row = load_wallet(&state, &owner, &wallet_id).await?;

    let proxy_address = row
        .proxy_address
        .unwrap_or_else(|| row.wallet_address.clone());

    // GET /status/{address} — path param, not query
    let resp = state
        .http
        .get(format!(
            "https://bridge.polymarket.com/status/{proxy_address}"
        ))
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Bridge API error: {e}")))?;

    if !resp.status().is_success() {
        return Ok(Json(DepositStatus { pending: vec![] }));
    }

    let data: serde_json::Value = resp.json().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            format!("Bridge API parse error: {e}"),
        )
    })?;

    let pending = data["transactions"]
        .as_array()
        .map(|txs| {
            txs.iter()
                .map(|tx| PendingDeposit {
                    from_chain: tx["fromChainId"].as_str().unwrap_or("unknown").to_string(),
                    token: tx["fromTokenAddress"]
                        .as_str()
                        .unwrap_or("unknown")
                        .to_string(),
                    amount: tx["fromAmountBaseUnit"].as_str().unwrap_or("0").to_string(),
                    status: tx["status"].as_str().unwrap_or("unknown").to_string(),
                    tx_hash: tx["txHash"].as_str().map(String::from),
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(Json(DepositStatus { pending }))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Loads a wallet by owner + ID, returning 404 if not found.
async fn load_wallet(
    state: &AppState,
    owner: &str,
    wallet_id: &str,
) -> Result<db::TradingWalletRow, (StatusCode, String)> {
    let state = state.clone();
    let owner = owner.to_string();
    let wallet_id = wallet_id.to_string();

    tokio::task::spawn_blocking(move || {
        let conn = state.user_db.lock().expect("user_db lock");
        db::get_trading_wallet_by_id(&conn, &owner, &wallet_id)
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or_else(|| (StatusCode::NOT_FOUND, "Trading wallet not found".into()))
}
