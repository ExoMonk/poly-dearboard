use alloy_primitives::{Address, B256, Signature};
use alloy_sol_types::{SolStruct, eip712_domain};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

alloy_sol_types::sol! {
    struct SignIn {
        address wallet;
        string nonce;
        string issuedAt;
    }
}

/// EIP-712 domain for PolyDerboard on Polygon.
fn domain() -> alloy_sol_types::Eip712Domain {
    eip712_domain! {
        name: "PolyDerboard",
        version: "1",
        chain_id: 137,
        verifying_contract: Address::ZERO,
    }
}

#[derive(Debug)]
pub enum AuthError {
    InvalidSignature,
    NonceMismatch,
    Expired,
    InvalidToken,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> axum::response::Response {
        let msg = match self {
            Self::InvalidSignature => "invalid signature",
            Self::NonceMismatch => "nonce mismatch",
            Self::Expired => "expired",
            Self::InvalidToken => "invalid token",
        };
        (StatusCode::UNAUTHORIZED, msg).into_response()
    }
}

/// Recovers the signer from an EIP-712 `SignIn` signature and verifies it matches `address`.
pub fn recover_eip712_signer(
    address: &str,
    nonce: &str,
    issued_at: &str,
    signature_hex: &str,
) -> Result<Address, AuthError> {
    let addr_lower = address.to_lowercase();

    // Parse the claimed address
    let claimed: Address = addr_lower
        .parse()
        .map_err(|_| AuthError::InvalidSignature)?;

    // Check issuedAt is within 5 minutes
    let issued: chrono::DateTime<chrono::Utc> =
        issued_at.parse().map_err(|_| AuthError::InvalidSignature)?;
    let age = chrono::Utc::now() - issued;
    if age.num_seconds() > 300 || age.num_seconds() < -60 {
        return Err(AuthError::Expired);
    }

    // Build the EIP-712 struct
    let sign_in = SignIn {
        wallet: claimed,
        nonce: nonce.to_string(),
        issuedAt: issued_at.to_string(),
    };

    // Compute signing hash: keccak256("\x19\x01" || domainSeparator || structHash)
    let signing_hash: B256 = sign_in.eip712_signing_hash(&domain());

    // Decode signature hex (strip 0x prefix if present)
    let sig_hex = signature_hex.strip_prefix("0x").unwrap_or(signature_hex);
    let sig_bytes = hex::decode(sig_hex).map_err(|_| AuthError::InvalidSignature)?;
    if sig_bytes.len() != 65 {
        return Err(AuthError::InvalidSignature);
    }

    // Parse 65-byte signature (r || s || v)
    let sig = Signature::from_raw(&sig_bytes).map_err(|_| AuthError::InvalidSignature)?;

    // Recover the signer address
    let recovered = sig
        .recover_address_from_prehash(&signing_hash)
        .map_err(|_| AuthError::InvalidSignature)?;

    if recovered != claimed {
        return Err(AuthError::InvalidSignature);
    }

    Ok(recovered)
}

#[derive(Serialize, Deserialize)]
struct Claims {
    sub: String,
    iat: u64,
    exp: u64,
}

/// Issues a JWT for the given wallet address (7-day expiry).
pub fn issue_jwt(address: &str, secret: &[u8]) -> String {
    let now = chrono::Utc::now().timestamp() as u64;
    let claims = Claims {
        sub: address.to_lowercase(),
        iat: now,
        exp: now + 7 * 24 * 3600,
    };
    jsonwebtoken::encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret),
    )
    .expect("JWT encoding failed")
}

/// Validates a JWT and returns the wallet address.
pub fn validate_jwt(token: &str, secret: &[u8]) -> Result<String, AuthError> {
    let data = jsonwebtoken::decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret),
        &Validation::default(),
    )
    .map_err(|_| AuthError::InvalidToken)?;
    Ok(data.claims.sub)
}
