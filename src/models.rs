use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::new_id;

#[derive(Debug, Clone)]
pub struct ClickEvent {
    pub id: String,
    pub link_id: String,
    pub clicked_at: NaiveDateTime,
    pub country: Option<String>,
    pub referrer: Option<String>,
}

impl ClickEvent {
    pub fn new(
        link_id: String,
        clicked_at: NaiveDateTime,
        country: Option<String>,
        referrer: Option<String>,
    ) -> Self {
        Self {
            id: new_id(),
            link_id,
            clicked_at,
            country,
            referrer,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Link {
    pub id: String,
    pub user_id: String,
    pub code: String,
    pub original_url: String,
    pub expires_at: Option<NaiveDateTime>,
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedLink {
    pub link_id: String,
    pub original_url: String,
    pub expires_at: Option<DateTime<Utc>>,
}

impl From<&Link> for CachedLink {
    fn from(link: &Link) -> Self {
        Self {
            link_id: link.id.clone(),
            original_url: link.original_url.clone(),
            expires_at: link.expires_at.map(|value| value.and_utc()),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct LinkResponse {
    pub code: String,
    pub original_url: String,
    pub short_url: String,
    pub expires_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct StatsResponse {
    pub code: String,
    pub total_clicks: i64,
    pub clicks_by_day: Vec<DayStat>,
    pub clicks_by_country: Vec<BucketStat>,
    pub clicks_by_referrer: Vec<BucketStat>,
}

#[derive(Debug, Serialize)]
pub struct DayStat {
    pub day: String,
    pub clicks: i64,
}

#[derive(Debug, Serialize)]
pub struct BucketStat {
    pub key: String,
    pub clicks: i64,
}

pub fn format_timestamp(value: NaiveDateTime) -> String {
    value.and_utc().to_rfc3339()
}

pub fn format_timestamp_opt(value: Option<NaiveDateTime>) -> Option<String> {
    value.map(format_timestamp)
}

pub fn short_url(public_base_url: &str, code: &str) -> String {
    format!("{}/{}", public_base_url.trim_end_matches('/'), code)
}

impl Link {
    pub fn to_response(&self, public_base_url: &str) -> LinkResponse {
        LinkResponse {
            code: self.code.clone(),
            original_url: self.original_url.clone(),
            short_url: short_url(public_base_url, &self.code),
            expires_at: format_timestamp_opt(self.expires_at),
            created_at: format_timestamp(self.created_at),
        }
    }

    pub fn is_expired(&self, now: NaiveDateTime) -> bool {
        self.expires_at.is_some_and(|expires_at| expires_at <= now)
    }
}
