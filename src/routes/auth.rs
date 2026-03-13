use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::auth::{self, hash_password, verify_password};
use crate::db;
use crate::domain::{new_id, normalize_email, validate_password};
use crate::error::AppError;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct Credentials {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub token: String,
}

pub async fn register(
    State(state): State<AppState>,
    Json(body): Json<Credentials>,
) -> Result<(StatusCode, Json<AuthResponse>), AppError> {
    let email = normalize_email(&body.email)?;
    validate_password(&body.password)?;
    let password_hash = hash_password(&body.password)?;
    let user_id = new_id();
    db::insert_user(
        &state.db,
        &user_id,
        &email,
        &password_hash,
        Utc::now().naive_utc(),
    )
    .await?;
    let token = auth_token(&state, &user_id)?;
    Ok((StatusCode::CREATED, Json(AuthResponse { token })))
}

pub async fn login(
    State(state): State<AppState>,
    Json(body): Json<Credentials>,
) -> Result<Json<AuthResponse>, AppError> {
    let email = normalize_email(&body.email)?;
    let Some(user) = db::find_user_by_email(&state.db, &email).await? else {
        return Err(AppError::Unauthorized);
    };
    if !verify_password(&body.password, &user.password_hash)? {
        return Err(AppError::Unauthorized);
    }
    let token = auth_token(&state, &user.id)?;
    Ok(Json(AuthResponse { token }))
}

fn auth_token(state: &AppState, user_id: &str) -> Result<String, AppError> {
    auth::issue_token(
        user_id,
        &state.config.jwt_secret,
        state.config.jwt_ttl.as_secs(),
    )
}
