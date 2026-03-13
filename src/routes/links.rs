use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::auth::AuthUser;
use crate::cache;
use crate::db;
use crate::domain::{ensure_future, new_id, random_code, validate_code, validate_url};
use crate::error::AppError;
use crate::models::{Link, LinkResponse, StatsResponse};
use crate::state::AppState;

const CODE_ATTEMPTS: usize = 5;
const GENERATED_CODE_LEN: usize = 7;

#[derive(Debug, Deserialize)]
pub struct CreateLinkRequest {
    pub url: String,
    pub custom_code: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

pub async fn create_link(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Json(body): Json<CreateLinkRequest>,
) -> Result<(StatusCode, Json<LinkResponse>), AppError> {
    let original_url = validate_url(&body.url)?;
    let now = Utc::now();
    ensure_future(body.expires_at, now)?;
    let custom = body
        .custom_code
        .as_deref()
        .map(str::trim)
        .filter(|code| !code.is_empty())
        .map(str::to_string);
    if let Some(code) = &custom {
        validate_code(code)?;
    }

    let mut last_conflict = None;
    for attempt in 0..CODE_ATTEMPTS {
        let code = match &custom {
            Some(code) => code.clone(),
            None => random_code(GENERATED_CODE_LEN),
        };
        let link = Link {
            id: new_id(),
            user_id: user_id.clone(),
            code,
            original_url: original_url.clone(),
            expires_at: body.expires_at.map(|value| value.naive_utc()),
            created_at: now.naive_utc(),
        };
        match db::insert_link(&state.db, &link).await {
            Ok(()) => {
                if let Err(err) = cache::invalidate(&state, &link.code).await {
                    tracing::warn!(error = %err, code = %link.code, "invalidation du cache après création");
                }
                return Ok((
                    StatusCode::CREATED,
                    Json(link.to_response(&state.config.public_base_url)),
                ));
            }
            Err(AppError::Conflict(message)) if custom.is_some() => {
                return Err(AppError::Conflict(message));
            }
            Err(AppError::Conflict(message)) => {
                last_conflict = Some(message);
                tracing::debug!(attempt, "collision d'alias, nouvel essai");
            }
            Err(err) => return Err(err),
        }
    }

    Err(AppError::conflict(last_conflict.unwrap_or_else(|| {
        "impossible de générer un alias".to_string()
    })))
}

pub async fn list_links(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
) -> Result<Json<Vec<LinkResponse>>, AppError> {
    let links = db::list_links_by_user(&state.db, &user_id).await?;
    let body = links
        .iter()
        .map(|link| link.to_response(&state.config.public_base_url))
        .collect();
    Ok(Json(body))
}

pub async fn link_stats(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Path(code): Path<String>,
) -> Result<Json<StatsResponse>, AppError> {
    let link = owned_link(&state, &user_id, &code).await?;
    let (total_clicks, clicks_by_day, clicks_by_country, clicks_by_referrer) =
        db::click_stats(&state.db, &link.id).await?;
    Ok(Json(StatsResponse {
        code: link.code,
        total_clicks,
        clicks_by_day,
        clicks_by_country,
        clicks_by_referrer,
    }))
}

pub async fn delete_link(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Path(code): Path<String>,
) -> Result<StatusCode, AppError> {
    let link = owned_link(&state, &user_id, &code).await?;
    db::delete_link(&state.db, &link.id, &user_id).await?;
    cache::invalidate(&state, &link.code).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn owned_link(state: &AppState, user_id: &str, code: &str) -> Result<Link, AppError> {
    let Some(link) = db::find_link_by_code(&state.db, code).await? else {
        return Err(AppError::NotFound);
    };
    if link.user_id != user_id {
        return Err(AppError::Forbidden);
    }
    Ok(link)
}
