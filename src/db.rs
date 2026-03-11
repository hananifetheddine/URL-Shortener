//! SQL MySQL avec des paramètres positionnels `?`.
//! Les requêtes sont exécutées avec `query` / `query_as` pour que
//! `cargo build` et l'image Docker n'aient pas besoin d'un serveur
//! MySQL au moment de la compilation.

use chrono::NaiveDateTime;
use sqlx::MySqlPool;

use crate::error::{AppError, is_mysql_duplicate};
use crate::models::{BucketStat, DayStat, Link};

#[derive(Debug, sqlx::FromRow)]
struct UserRow {
    id: String,
    password_hash: String,
}

#[derive(Debug, sqlx::FromRow)]
pub struct UserAuth {
    pub id: String,
    pub password_hash: String,
}

#[derive(Debug, sqlx::FromRow)]
struct LinkRow {
    id: String,
    user_id: String,
    code: String,
    original_url: String,
    expires_at: Option<NaiveDateTime>,
    created_at: NaiveDateTime,
}

#[derive(Debug, sqlx::FromRow)]
struct DayRow {
    day: String,
    total: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct BucketRow {
    bucket: String,
    total: i64,
}

impl From<LinkRow> for Link {
    fn from(row: LinkRow) -> Self {
        Self {
            id: row.id,
            user_id: row.user_id,
            code: row.code,
            original_url: row.original_url,
            expires_at: row.expires_at,
            created_at: row.created_at,
        }
    }
}

pub async fn insert_user(
    pool: &MySqlPool,
    id: &str,
    email: &str,
    password_hash: &str,
    created_at: NaiveDateTime,
) -> Result<(), AppError> {
    let result =
        sqlx::query("INSERT INTO users (id, email, password_hash, created_at) VALUES (?, ?, ?, ?)")
            .bind(id)
            .bind(email)
            .bind(password_hash)
            .bind(created_at)
            .execute(pool)
            .await;

    match result {
        Ok(_) => Ok(()),
        Err(err) if is_mysql_duplicate(&err) => {
            Err(AppError::conflict("cet email est déjà utilisé"))
        }
        Err(err) => Err(err.into()),
    }
}

pub async fn find_user_by_email(
    pool: &MySqlPool,
    email: &str,
) -> Result<Option<UserAuth>, AppError> {
    let row =
        sqlx::query_as::<_, UserRow>("SELECT id, password_hash FROM users WHERE email = ? LIMIT 1")
            .bind(email)
            .fetch_optional(pool)
            .await?;

    Ok(row.map(|row| UserAuth {
        id: row.id,
        password_hash: row.password_hash,
    }))
}

pub async fn insert_link(pool: &MySqlPool, link: &Link) -> Result<(), AppError> {
    let result = sqlx::query(
        "INSERT INTO links (id, user_id, code, original_url, expires_at, created_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&link.id)
    .bind(&link.user_id)
    .bind(&link.code)
    .bind(&link.original_url)
    .bind(link.expires_at)
    .bind(link.created_at)
    .execute(pool)
    .await;

    match result {
        Ok(_) => Ok(()),
        Err(err) if is_mysql_duplicate(&err) => {
            Err(AppError::conflict("cet alias est déjà utilisé"))
        }
        Err(err) => Err(err.into()),
    }
}

pub async fn find_link_by_code(pool: &MySqlPool, code: &str) -> Result<Option<Link>, AppError> {
    let row = sqlx::query_as::<_, LinkRow>(
        "SELECT id, user_id, code, original_url, expires_at, created_at
         FROM links
         WHERE code = ?
         LIMIT 1",
    )
    .bind(code)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(Link::from))
}

pub async fn list_links_by_user(pool: &MySqlPool, user_id: &str) -> Result<Vec<Link>, AppError> {
    let rows = sqlx::query_as::<_, LinkRow>(
        "SELECT id, user_id, code, original_url, expires_at, created_at
         FROM links
         WHERE user_id = ?
         ORDER BY created_at DESC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Link::from).collect())
}

pub async fn delete_link(pool: &MySqlPool, id: &str, user_id: &str) -> Result<(), AppError> {
    let result = sqlx::query("DELETE FROM links WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

pub async fn click_stats(
    pool: &MySqlPool,
    link_id: &str,
) -> Result<(i64, Vec<DayStat>, Vec<BucketStat>, Vec<BucketStat>), AppError> {
    let total = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM clicks WHERE link_id = ?")
        .bind(link_id)
        .fetch_one(pool)
        .await?;

    let days = sqlx::query_as::<_, DayRow>(
        "SELECT DATE_FORMAT(clicked_at, '%Y-%m-%d') AS day, COUNT(*) AS total
         FROM clicks
         WHERE link_id = ?
         GROUP BY DATE_FORMAT(clicked_at, '%Y-%m-%d')
         ORDER BY day",
    )
    .bind(link_id)
    .fetch_all(pool)
    .await?;

    let countries = sqlx::query_as::<_, BucketRow>(
        "SELECT COALESCE(country, 'unknown') AS bucket, COUNT(*) AS total
         FROM clicks
         WHERE link_id = ?
         GROUP BY COALESCE(country, 'unknown')
         ORDER BY total DESC",
    )
    .bind(link_id)
    .fetch_all(pool)
    .await?;

    let referrers = sqlx::query_as::<_, BucketRow>(
        "SELECT COALESCE(referrer, 'direct') AS bucket, COUNT(*) AS total
         FROM clicks
         WHERE link_id = ?
         GROUP BY COALESCE(referrer, 'direct')
         ORDER BY total DESC
         LIMIT 20",
    )
    .bind(link_id)
    .fetch_all(pool)
    .await?;

    Ok((
        total,
        days.into_iter()
            .map(|row| DayStat {
                day: row.day,
                clicks: row.total,
            })
            .collect(),
        countries
            .into_iter()
            .map(|row| BucketStat {
                key: row.bucket,
                clicks: row.total,
            })
            .collect(),
        referrers
            .into_iter()
            .map(|row| BucketStat {
                key: row.bucket,
                clicks: row.total,
            })
            .collect(),
    ))
}

pub async fn insert_click_batch(
    pool: &MySqlPool,
    clicks: &[crate::models::ClickEvent],
) -> Result<(), sqlx::Error> {
    if clicks.is_empty() {
        return Ok(());
    }

    let mut builder = sqlx::QueryBuilder::<sqlx::MySql>::new(
        "INSERT INTO clicks (id, link_id, clicked_at, country, referrer) ",
    );
    builder.push_values(clicks, |mut row, click| {
        row.push_bind(&click.id)
            .push_bind(&click.link_id)
            .push_bind(click.clicked_at)
            .push_bind(&click.country)
            .push_bind(&click.referrer);
    });
    builder.build().execute(pool).await?;
    Ok(())
}
