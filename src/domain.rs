use std::time::Duration;

use chrono::{DateTime, NaiveDateTime, Utc};

use crate::error::AppError;

const CODE_ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const RESERVED_CODES: &[&str] = &[
    "auth", "links", "health", "metrics", "ready", "live", "static", "api",
];

pub fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

pub fn random_code(len: usize) -> String {
    (0..len)
        .map(|_| {
            let index = rand::random_range(0..CODE_ALPHABET.len());
            CODE_ALPHABET[index] as char
        })
        .collect()
}

pub fn validate_code(code: &str) -> Result<(), AppError> {
    if !(3..=32).contains(&code.len()) {
        return Err(AppError::bad_request(
            "l'alias doit contenir entre 3 et 32 caractères",
        ));
    }
    if !code
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '-' || character == '_')
    {
        return Err(AppError::bad_request(
            "l'alias ne peut contenir que des lettres, des chiffres, '-' et '_'",
        ));
    }
    if RESERVED_CODES
        .iter()
        .any(|reserved| code.eq_ignore_ascii_case(reserved))
    {
        return Err(AppError::bad_request("cet alias est réservé"));
    }
    Ok(())
}

pub fn validate_url(raw: &str) -> Result<String, AppError> {
    let url = raw.trim();
    if url.is_empty() || url.len() > 2048 {
        return Err(AppError::bad_request(
            "l'URL doit contenir entre 1 et 2048 caractères",
        ));
    }
    let parsed = url::Url::parse(url).map_err(|_| AppError::bad_request("URL invalide"))?;
    match parsed.scheme() {
        "http" | "https" => Ok(url.to_string()),
        _ => Err(AppError::bad_request(
            "seuls les schémas http et https sont acceptés",
        )),
    }
}

pub fn normalize_email(raw: &str) -> Result<String, AppError> {
    let email = raw.trim().to_lowercase();
    if email.len() < 3 || email.len() > 255 {
        return Err(AppError::bad_request("email invalide"));
    }
    let Some((local, domain)) = email.split_once('@') else {
        return Err(AppError::bad_request("email invalide"));
    };
    if local.is_empty()
        || domain.len() < 3
        || !domain.contains('.')
        || domain.starts_with('.')
        || domain.ends_with('.')
        || domain.contains("..")
    {
        return Err(AppError::bad_request("email invalide"));
    }
    Ok(email)
}

pub fn validate_password(password: &str) -> Result<(), AppError> {
    let bytes = password.len();
    let chars = password.chars().count();
    if chars < 8 || bytes > 128 {
        return Err(AppError::bad_request(
            "le mot de passe doit contenir entre 8 et 128 caractères",
        ));
    }
    Ok(())
}

pub fn ensure_future(
    expires_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Result<(), AppError> {
    if expires_at.is_some_and(|expires_at| expires_at <= now) {
        return Err(AppError::bad_request(
            "la date d'expiration est déjà passée",
        ));
    }
    Ok(())
}

/// TTL du cache : le minimum entre le TTL configuré et le temps restant avant expiration.
pub fn cache_ttl_secs(
    expires_at: Option<NaiveDateTime>,
    default_ttl: Duration,
    now: NaiveDateTime,
) -> u64 {
    let default_secs = default_ttl.as_secs().max(1);
    match expires_at {
        Some(expires_at) => {
            let remaining = (expires_at - now).num_seconds();
            if remaining <= 0 {
                1
            } else {
                (remaining as u64).min(default_secs)
            }
        }
        None => default_secs,
    }
}

pub fn normalize_country(raw: &str) -> Option<String> {
    let value = raw.trim();
    if value.is_empty() || value.eq_ignore_ascii_case("xx") || value == "T1" {
        return None;
    }
    if (2..=8).contains(&value.len())
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
    {
        Some(value.to_ascii_uppercase())
    } else {
        None
    }
}

pub fn normalize_referrer(raw: &str) -> Option<String> {
    let value = raw.trim();
    if value.is_empty() {
        return None;
    }
    let truncated: String = value.chars().take(1024).collect();
    Some(truncated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_http_and_https_only() {
        assert!(validate_url("https://example.com/a").is_ok());
        assert!(validate_url("http://example.com").is_ok());
        assert!(validate_url("javascript:alert(1)").is_err());
        assert!(validate_url("ftp://example.com").is_err());
    }

    #[test]
    fn rejects_reserved_and_short_codes() {
        assert!(validate_code("ab").is_err());
        assert!(validate_code("docs").is_ok());
        assert!(validate_code("links").is_err());
        assert!(validate_code("HEALTH").is_err());
        assert!(validate_code("a b").is_err());
        assert!(validate_code("été").is_err());
    }

    #[test]
    fn cache_ttl_is_capped_by_expiry() {
        let now = NaiveDateTime::parse_from_str("2026-01-01 00:00:00", "%Y-%m-%d %H:%M:%S")
            .expect("date");
        let soon = now + chrono::Duration::seconds(30);
        assert_eq!(
            cache_ttl_secs(Some(soon), Duration::from_secs(300), now),
            30
        );
        assert_eq!(cache_ttl_secs(None, Duration::from_secs(300), now), 300);
        assert_eq!(cache_ttl_secs(Some(now), Duration::from_secs(300), now), 1);
    }

    #[test]
    fn normalizes_identity_fields() {
        assert_eq!(
            normalize_email("  Ada@Example.COM ").expect("email"),
            "ada@example.com"
        );
        assert!(normalize_email("ada").is_err());
        assert!(validate_password("short").is_err());
        assert!(validate_password("long-enough").is_ok());
        assert_eq!(normalize_country(" fr "), Some("FR".to_string()));
        assert_eq!(normalize_country("xx"), None);
        assert_eq!(normalize_referrer("  "), None);
    }
}
