use std::env;
use std::time::Duration;

use thiserror::Error;

#[derive(Clone)]
pub struct Config {
    pub database_url: String,
    pub redis_url: String,
    pub redis_prefix: String,
    pub jwt_secret: String,
    pub jwt_ttl: Duration,
    pub bind_addr: String,
    pub public_base_url: String,
    pub cache_ttl: Duration,
    pub click_channel_capacity: usize,
    pub click_batch_size: usize,
    pub click_flush_interval: Duration,
    pub rate_limit_ip_per_minute: u64,
    pub rate_limit_user_per_minute: u64,
    pub trust_proxy: bool,
    pub db_max_connections: u32,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("variable d'environnement {0} manquante")]
    Missing(&'static str),
    #[error("variable d'environnement {name} invalide : {reason}")]
    Invalid { name: &'static str, reason: String },
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let jwt_secret = required("JWT_SECRET")?;
        if jwt_secret.len() < 32 {
            return Err(ConfigError::Invalid {
                name: "JWT_SECRET",
                reason: "32 caractères minimum".to_string(),
            });
        }

        let jwt_ttl_secs = optional_parse("JWT_TTL_SECS", 86_400)?;
        if jwt_ttl_secs == 0 {
            return Err(ConfigError::Invalid {
                name: "JWT_TTL_SECS",
                reason: "doit être supérieur à 0".to_string(),
            });
        }

        let click_batch_size = optional_parse("CLICK_BATCH_SIZE", 100)?;
        if click_batch_size == 0 {
            return Err(ConfigError::Invalid {
                name: "CLICK_BATCH_SIZE",
                reason: "doit être supérieur à 0".to_string(),
            });
        }

        let click_channel_capacity = optional_parse("CLICK_CHANNEL_CAPACITY", 10_000)?;
        if click_channel_capacity == 0 {
            return Err(ConfigError::Invalid {
                name: "CLICK_CHANNEL_CAPACITY",
                reason: "doit être supérieur à 0".to_string(),
            });
        }

        let click_flush_interval_ms = optional_parse("CLICK_FLUSH_INTERVAL_MS", 500)?;
        if click_flush_interval_ms == 0 {
            return Err(ConfigError::Invalid {
                name: "CLICK_FLUSH_INTERVAL_MS",
                reason: "doit être supérieur à 0".to_string(),
            });
        }

        let db_max_connections = optional_parse("DB_MAX_CONNECTIONS", 10)?;
        if db_max_connections == 0 {
            return Err(ConfigError::Invalid {
                name: "DB_MAX_CONNECTIONS",
                reason: "doit être supérieur à 0".to_string(),
            });
        }

        Ok(Self {
            database_url: required("DATABASE_URL")?,
            redis_url: required("REDIS_URL")?,
            redis_prefix: optional_string("REDIS_KEY_PREFIX", "urlshort"),
            jwt_secret,
            jwt_ttl: Duration::from_secs(jwt_ttl_secs),
            bind_addr: optional_string("BIND_ADDR", "127.0.0.1:8080"),
            public_base_url: optional_string("PUBLIC_BASE_URL", "http://127.0.0.1:8080"),
            cache_ttl: Duration::from_secs(optional_parse("CACHE_TTL_SECS", 300)?),
            click_channel_capacity,
            click_batch_size,
            click_flush_interval: Duration::from_millis(click_flush_interval_ms),
            rate_limit_ip_per_minute: optional_parse("RATE_LIMIT_IP_PER_MINUTE", 120)?,
            rate_limit_user_per_minute: optional_parse("RATE_LIMIT_USER_PER_MINUTE", 300)?,
            trust_proxy: optional_bool("TRUST_PROXY", false)?,
            db_max_connections,
        })
    }
}

fn required(name: &'static str) -> Result<String, ConfigError> {
    match env::var(name) {
        Ok(value) if !value.trim().is_empty() => Ok(value),
        Ok(_) | Err(env::VarError::NotPresent) => Err(ConfigError::Missing(name)),
        Err(env::VarError::NotUnicode(_)) => Err(ConfigError::Invalid {
            name,
            reason: "la valeur n'est pas de l'Unicode".to_string(),
        }),
    }
}

fn optional_string(name: &str, default: &str) -> String {
    env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn optional_parse<T>(name: &'static str, default: T) -> Result<T, ConfigError>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match env::var(name) {
        Ok(value) if value.trim().is_empty() => Ok(default),
        Ok(value) => value.parse().map_err(|err: T::Err| ConfigError::Invalid {
            name,
            reason: err.to_string(),
        }),
        Err(env::VarError::NotPresent) => Ok(default),
        Err(env::VarError::NotUnicode(_)) => Err(ConfigError::Invalid {
            name,
            reason: "la valeur n'est pas de l'Unicode".to_string(),
        }),
    }
}

fn optional_bool(name: &'static str, default: bool) -> Result<bool, ConfigError> {
    match env::var(name) {
        Ok(value) if value.trim().is_empty() => Ok(default),
        Ok(value) => match value.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" => Ok(true),
            "0" | "false" | "no" => Ok(false),
            _ => Err(ConfigError::Invalid {
                name,
                reason: "attendu : true ou false".to_string(),
            }),
        },
        Err(env::VarError::NotPresent) => Ok(default),
        Err(env::VarError::NotUnicode(_)) => Err(ConfigError::Invalid {
            name,
            reason: "la valeur n'est pas de l'Unicode".to_string(),
        }),
    }
}
