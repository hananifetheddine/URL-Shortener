use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::Response;
use chrono::Utc;

use crate::cache;
use crate::domain::{normalize_country, normalize_referrer};
use crate::error::AppError;
use crate::models::CachedLink;
use crate::models::ClickEvent;
use crate::state::AppState;

pub async fn redirect(
    State(state): State<AppState>,
    Path(code): Path<String>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let Some(link) = cache::resolve(&state, &code).await? else {
        return Err(AppError::NotFound);
    };
    if is_expired(&link) {
        return Err(AppError::NotFound);
    }

    let country = headers
        .get("cf-ipcountry")
        .or_else(|| headers.get("x-country-code"))
        .and_then(|value| value.to_str().ok())
        .and_then(normalize_country);
    let referrer = headers
        .get(header::REFERER)
        .and_then(|value| value.to_str().ok())
        .and_then(normalize_referrer);
    let event = ClickEvent::new(
        link.link_id.clone(),
        Utc::now().naive_utc(),
        country,
        referrer,
    );
    if let Err(err) = state.click_tx.try_send(event) {
        tracing::warn!(error = %err, "clic non enregistré : file d'attente saturée ou fermée");
    }

    let location = HeaderValue::from_str(&link.original_url).map_err(AppError::internal)?;
    let mut response = Response::new(axum::body::Body::empty());
    *response.status_mut() = StatusCode::FOUND;
    response.headers_mut().insert(header::LOCATION, location);
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

fn is_expired(link: &CachedLink) -> bool {
    link.expires_at
        .is_some_and(|expires_at| expires_at <= Utc::now())
}
