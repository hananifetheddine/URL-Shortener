mod auth;
mod cache;
mod clicks;
mod config;
mod db;
mod domain;
mod error;
mod models;
mod rate_limit;
mod routes;
mod state;

pub use config::{Config, ConfigError};
pub use error::AppError;
pub use state::{AppContext, AppState};

pub fn router(state: AppState) -> axum::Router {
    routes::router(state)
}
