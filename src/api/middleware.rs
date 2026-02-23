use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::http::request::Parts;

use super::server::AppState;

/// Validates and normalizes an Ethereum address (0x + 40 hex chars).
pub fn validate_eth_address(s: &str) -> Result<String, StatusCode> {
    let lower = s.to_lowercase();
    if lower.len() == 42
        && lower.starts_with("0x")
        && lower[2..].chars().all(|c| c.is_ascii_hexdigit())
    {
        Ok(lower)
    } else {
        Err(StatusCode::BAD_REQUEST)
    }
}

/// Extracted wallet address from a validated JWT.
pub struct AuthUser(pub String);

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = StatusCode;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or(StatusCode::UNAUTHORIZED)?;

        let token = header
            .strip_prefix("Bearer ")
            .ok_or(StatusCode::UNAUTHORIZED)?;

        let address = super::auth::validate_jwt(token, &state.jwt_secret)
            .map_err(|_| StatusCode::UNAUTHORIZED)?;

        Ok(AuthUser(address))
    }
}
