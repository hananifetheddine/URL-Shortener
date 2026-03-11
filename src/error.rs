use axum::Json;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use thiserror::Error;

pub type BoxedError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(String),
    #[error("authentification requise")]
    Unauthorized,
    #[error("accès refusé")]
    Forbidden,
    #[error("ressource introuvable")]
    NotFound,
    #[error("{0}")]
    Conflict(String),
    #[error("trop de requêtes")]
    TooManyRequests,
    #[error("service indisponible")]
    Unavailable,
    #[error("erreur interne")]
    Internal(#[source] BoxedError),
    #[error("base de données")]
    Database(#[source] Box<sqlx::Error>),
    #[error("cache")]
    Redis(#[source] Box<redis::RedisError>),
    #[error("migration")]
    Migrate(#[source] Box<sqlx::migrate::MigrateError>),
}

impl AppError {
    pub fn internal(err: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Internal(Box::new(err))
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::BadRequest(message.into())
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::Conflict(message.into())
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        Self::Database(Box::new(err))
    }
}

impl From<redis::RedisError> for AppError {
    fn from(err: redis::RedisError) -> Self {
        Self::Redis(Box::new(err))
    }
}

impl From<sqlx::migrate::MigrateError> for AppError {
    fn from(err: sqlx::migrate::MigrateError) -> Self {
        Self::Migrate(Box::new(err))
    }
}

pub fn is_mysql_duplicate(err: &sqlx::Error) -> bool {
    match err {
        sqlx::Error::Database(db) => db.code().as_deref() == Some("1062"),
        _ => false,
    }
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    error: &'a str,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            Self::BadRequest(message) => (StatusCode::BAD_REQUEST, message.clone()),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, self.to_string()),
            Self::Forbidden => (StatusCode::FORBIDDEN, self.to_string()),
            Self::NotFound => (StatusCode::NOT_FOUND, self.to_string()),
            Self::Conflict(message) => (StatusCode::CONFLICT, message.clone()),
            Self::TooManyRequests => (StatusCode::TOO_MANY_REQUESTS, self.to_string()),
            Self::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, self.to_string()),
            Self::Internal(_) | Self::Database(_) | Self::Redis(_) | Self::Migrate(_) => {
                tracing::error!(error = %self, "requête en échec");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "erreur interne".to_string(),
                )
            }
        };

        let mut response = (status, Json(ErrorBody { error: &message })).into_response();
        if status == StatusCode::TOO_MANY_REQUESTS {
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from_static("60"));
        }
        response
    }
}
