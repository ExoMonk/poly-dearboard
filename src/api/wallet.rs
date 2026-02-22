use alloy::signers::Signer as _;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    Json,
};
use secrecy::ExposeSecret;
use std::str::FromStr;

use super::db::{self, WalletError};
use super::middleware::AuthUser;
use super::server::AppState;
use super::types::{
    DeriveCredentialsResponse, ImportWalletRequest, ImportWalletResponse, TradingWalletInfo,
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
    let signing_key = k256::ecdsa::SigningKey::random(
        &mut k256::elliptic_curve::rand_core::OsRng,
    );
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
            db::create_trading_wallet(&conn, &owner, &wallet_addr, &proxy_addr, &encrypted_key, &key_nonce)
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
        k256::ecdsa::SigningKey::from_bytes(key_bytes.as_slice().into())
            .map_err(|_| {
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
            db::create_trading_wallet(&conn, &owner, &wallet_addr, &proxy_addr, &encrypted_key, &key_nonce)
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
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Decryption failed: {e}")))?;

    let private_key_hex = format!("0x{}", hex::encode(&private_key_bytes));

    // 3. Create signer and derive CLOB credentials via SDK
    let signer = alloy::signers::local::LocalSigner::from_str(&private_key_hex)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Signer creation failed: {e}")))?
        .with_chain_id(Some(polymarket_client_sdk::POLYGON));

    let config = polymarket_client_sdk::clob::Config::builder()
        .use_server_time(true)
        .build();

    let clob_client = polymarket_client_sdk::clob::Client::new("https://clob.polymarket.com", config)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("CLOB client error: {e}")))?;

    let credentials = clob_client
        .create_or_derive_api_key(&signer, None)
        .await
        .map_err(|e| (StatusCode::SERVICE_UNAVAILABLE, format!("CLOB API error: {e}")))?;

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
            db::update_wallet_credentials(&conn, &owner, &wallet_id, &api_key, &cred_blob, &cred_nonce)
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

    // TODO(spec-15): Check for active copy-trade sessions before allowing delete
    // For now, just delete.

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
