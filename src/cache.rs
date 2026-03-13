use chrono::Utc;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};

use crate::domain::cache_ttl_secs;
use crate::error::AppError;
use crate::models::{CachedLink, Link};
use crate::state::AppState;

const NEGATIVE_TTL_SECS: u64 = 30;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CacheEntry {
    Hit(CachedLink),
    Miss,
}

enum CacheRead {
    Hit(CachedLink),
    Miss,
    Absent,
}

pub async fn resolve(state: &AppState, code: &str) -> Result<Option<CachedLink>, AppError> {
    match read(state, code).await {
        Ok(CacheRead::Hit(link)) => return Ok(Some(link)),
        Ok(CacheRead::Miss) => return Ok(None),
        Ok(CacheRead::Absent) => {}
        Err(err) => {
            tracing::warn!(error = %err, "lecture du cache impossible, repli sur MySQL");
        }
    }

    match crate::db::find_link_by_code(&state.db, code).await? {
        Some(link) if link.is_expired(Utc::now().naive_utc()) => {
            if let Err(err) = write_miss(state, code).await {
                tracing::warn!(error = %err, "écriture du cache négatif impossible");
            }
            Ok(None)
        }
        Some(link) => {
            let cached = CachedLink::from(&link);
            if let Err(err) = write_hit(state, code, &link, &cached).await {
                tracing::warn!(error = %err, "écriture du cache impossible");
            }
            Ok(Some(cached))
        }
        None => {
            if let Err(err) = write_miss(state, code).await {
                tracing::warn!(error = %err, "écriture du cache négatif impossible");
            }
            Ok(None)
        }
    }
}

pub async fn invalidate(state: &AppState, code: &str) -> Result<(), AppError> {
    let mut connection = state.redis.clone();
    let key = redirect_key(state, code);
    let _: () = connection.del(key).await?;
    Ok(())
}

async fn read(state: &AppState, code: &str) -> Result<CacheRead, AppError> {
    let mut connection = state.redis.clone();
    let key = redirect_key(state, code);
    let payload: Option<String> = connection.get(key).await?;
    let Some(payload) = payload else {
        return Ok(CacheRead::Absent);
    };
    match serde_json::from_str::<CacheEntry>(&payload) {
        Ok(CacheEntry::Hit(link)) => Ok(CacheRead::Hit(link)),
        Ok(CacheEntry::Miss) => Ok(CacheRead::Miss),
        Err(err) => {
            tracing::warn!(error = %err, "entrée de cache illisible, ignorée");
            Ok(CacheRead::Absent)
        }
    }
}

async fn write_hit(
    state: &AppState,
    code: &str,
    link: &Link,
    cached: &CachedLink,
) -> Result<(), AppError> {
    let ttl = cache_ttl_secs(
        link.expires_at,
        state.config.cache_ttl,
        Utc::now().naive_utc(),
    );
    store(state, code, &CacheEntry::Hit(cached.clone()), ttl).await
}

async fn write_miss(state: &AppState, code: &str) -> Result<(), AppError> {
    store(state, code, &CacheEntry::Miss, NEGATIVE_TTL_SECS).await
}

async fn store(
    state: &AppState,
    code: &str,
    entry: &CacheEntry,
    ttl_secs: u64,
) -> Result<(), AppError> {
    let payload = serde_json::to_string(entry).map_err(AppError::internal)?;
    let mut connection = state.redis.clone();
    let key = redirect_key(state, code);
    let _: () = connection.set_ex(key, payload, ttl_secs.max(1)).await?;
    Ok(())
}

fn redirect_key(state: &AppState, code: &str) -> String {
    format!("{}:redir:{}", state.config.redis_prefix, code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_entries_roundtrip() {
        let hit = CacheEntry::Hit(CachedLink {
            link_id: "018f0b3e-7b3a-7c2e-8a1a-1b2c3d4e5f60".to_string(),
            original_url: "https://example.com/docs".to_string(),
            expires_at: None,
        });
        let encoded = serde_json::to_string(&hit).expect("json");
        match serde_json::from_str::<CacheEntry>(&encoded).expect("parse") {
            CacheEntry::Hit(link) => {
                assert_eq!(link.original_url, "https://example.com/docs");
                assert!(link.expires_at.is_none());
            }
            CacheEntry::Miss => panic!("entrée positive attendue"),
        }

        let encoded = serde_json::to_string(&CacheEntry::Miss).expect("json");
        assert!(matches!(
            serde_json::from_str::<CacheEntry>(&encoded).expect("parse"),
            CacheEntry::Miss
        ));
    }
}
