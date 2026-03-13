use std::net::SocketAddr;

use axum::extract::{ConnectInfo, Request, State};
use axum::http::header::AUTHORIZATION;
use axum::middleware::Next;
use axum::response::Response;

use crate::auth::verify_token;
use crate::error::AppError;
use crate::state::AppState;

const WINDOW_SECS: u64 = 60;
const SCRIPT: &str = r#"
local current = redis.call('INCR', KEYS[1])
local ttl = redis.call('TTL', KEYS[1])
if ttl < 0 then
  redis.call('EXPIRE', KEYS[1], ARGV[1])
end
return current
"#;

pub async fn middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, AppError> {
    let ip = client_ip(&request, state.config.trust_proxy);
    enforce(
        &state,
        &format!("{}:rl:ip:{ip}", state.config.redis_prefix),
        state.config.rate_limit_ip_per_minute,
    )
    .await?;

    if let Some(user_id) = bearer_user(&request, &state.config.jwt_secret) {
        enforce(
            &state,
            &format!("{}:rl:user:{user_id}", state.config.redis_prefix),
            state.config.rate_limit_user_per_minute,
        )
        .await?;
    }

    Ok(next.run(request).await)
}

async fn enforce(state: &AppState, key: &str, limit: u64) -> Result<(), AppError> {
    if limit == 0 {
        return Ok(());
    }
    let mut connection = state.redis.clone();
    let current: i64 = redis::Script::new(SCRIPT)
        .key(key)
        .arg(WINDOW_SECS)
        .invoke_async(&mut connection)
        .await?;
    if current > i64::try_from(limit).unwrap_or(i64::MAX) {
        return Err(AppError::TooManyRequests);
    }
    Ok(())
}

fn bearer_user(request: &Request, secret: &str) -> Option<String> {
    let header = request.headers().get(AUTHORIZATION)?.to_str().ok()?;
    let token = header.strip_prefix("Bearer ")?.trim();
    if token.is_empty() {
        return None;
    }
    verify_token(token, secret).ok()
}

fn client_ip(request: &Request, trust_proxy: bool) -> String {
    if trust_proxy && let Some(ip) = forwarded_for(request) {
        return ip;
    }
    request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0.ip().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn forwarded_for(request: &Request) -> Option<String> {
    let raw = request.headers().get("x-forwarded-for")?.to_str().ok()?;
    let ip = raw.split(',').next()?.trim();
    if ip.is_empty() {
        None
    } else {
        Some(ip.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request as HttpRequest;

    #[test]
    fn ignores_forwarded_for_unless_trusted() {
        let request = HttpRequest::builder()
            .header("x-forwarded-for", "203.0.113.8, 10.0.0.1")
            .body(Body::empty())
            .expect("request");
        assert_eq!(client_ip(&request, false), "unknown");
        assert_eq!(client_ip(&request, true), "203.0.113.8");
    }
}
