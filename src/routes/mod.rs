mod auth;
mod links;
mod redirect;

use axum::Router;
use axum::middleware;
use axum::routing::{delete, get, post};
use tower_http::trace::TraceLayer;

use crate::rate_limit;
use crate::state::AppState;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/auth/register", post(auth::register))
        .route("/auth/login", post(auth::login))
        .route("/links", post(links::create_link).get(links::list_links))
        .route("/links/{code}/stats", get(links::link_stats))
        .route("/links/{code}", delete(links::delete_link))
        .route("/{code}", get(redirect::redirect))
        .with_state(state.clone())
        .layer(middleware::from_fn_with_state(
            state,
            rate_limit::middleware,
        ))
        .layer(TraceLayer::new_for_http())
}

async fn health() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({ "status": "ok" }))
}
