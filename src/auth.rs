use std::time::{SystemTime, UNIX_EPOCH};

use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};
use axum::extract::FromRequestParts;
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::state::AppState;

pub fn hash_password(password: &str) -> Result<String, AppError> {
    let hash = Argon2::default()
        .hash_password(password.as_bytes())
        .map_err(AppError::internal)?;
    Ok(hash.to_string())
}

pub fn verify_password(password: &str, encoded: &str) -> Result<bool, AppError> {
    let parsed = PasswordHash::new(encoded).map_err(AppError::internal)?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    iat: u64,
    exp: u64,
}

pub fn issue_token(user_id: &str, secret: &str, ttl_secs: u64) -> Result<String, AppError> {
    let now = unix_now();
    let claims = Claims {
        sub: user_id.to_string(),
        iat: now,
        exp: now.saturating_add(ttl_secs),
    };
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(AppError::internal)
}

pub fn verify_token(token: &str, secret: &str) -> Result<String, AppError> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map_err(|_| AppError::Unauthorized)?;
    if data.claims.sub.is_empty() {
        return Err(AppError::Unauthorized);
    }
    Ok(data.claims.sub)
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[derive(Debug)]
pub struct AuthUser(pub String);

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .ok_or(AppError::Unauthorized)?;
        let token = header
            .strip_prefix("Bearer ")
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .ok_or(AppError::Unauthorized)?;
        let user_id = verify_token(token, &state.config.jwt_secret)?;
        Ok(Self(user_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_roundtrip() {
        let hash = hash_password("correct-horse").expect("hash");
        assert!(verify_password("correct-horse", &hash).expect("verify"));
        assert!(!verify_password("wrong-horse", &hash).expect("verify"));
    }

    #[test]
    fn jwt_roundtrip() {
        let secret = "test-secret-that-is-at-least-32-chars";
        let token = issue_token("user-1", secret, 60).expect("token");
        assert_eq!(verify_token(&token, secret).expect("sub"), "user-1");
        assert!(verify_token(&token, "another-secret-that-is-at-least-32").is_err());
    }
}
