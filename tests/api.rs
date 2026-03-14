use std::net::SocketAddr;
use std::time::Duration;

use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use url_shortener::{AppContext, Config, router};

struct Running {
    base: String,
    server: JoinHandle<()>,
    context: AppContext,
}

impl Running {
    async fn start(mut config: Config) -> Self {
        config.bind_addr = "127.0.0.1:0".to_string();
        let context = AppContext::connect(config)
            .await
            .expect("connexion MySQL et Redis");
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let state = context.state.clone();
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                router(state).into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .expect("serveur");
        });
        Self {
            base: format!("http://{addr}"),
            server,
            context,
        }
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.server.abort();
        self.context.stop();
    }
}

fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("client http")
}

fn test_config() -> Config {
    dotenvy::dotenv().ok();
    let mut config = Config::from_env().expect("variables d'environnement de test");
    config.rate_limit_ip_per_minute = 10_000;
    config.rate_limit_user_per_minute = 10_000;
    config.click_batch_size = 1;
    config.click_flush_interval = Duration::from_millis(50);
    config.redis_prefix = format!("urlshort-test-{}", uuid::Uuid::now_v7());
    config
}

fn unique_email() -> String {
    format!("user-{}@example.com", uuid::Uuid::now_v7())
}

fn short_code(prefix: &str) -> String {
    let suffix = uuid::Uuid::now_v7().simple().to_string();
    format!("{prefix}{}", &suffix[..8])
}

async fn register(base: &str, email: &str, password: &str) -> String {
    let response = http()
        .post(format!("{base}/auth/register"))
        .json(&serde_json::json!({
            "email": email,
            "password": password
        }))
        .send()
        .await
        .expect("register");
    assert_eq!(response.status(), reqwest::StatusCode::CREATED);
    let body: serde_json::Value = response.json().await.expect("json");
    body["token"].as_str().expect("token").to_string()
}

#[tokio::test]
async fn register_login_and_rejects_bad_credentials() {
    let app = Running::start(test_config()).await;
    let email = unique_email();
    let token = register(&app.base, &email, "password-123").await;
    assert!(!token.is_empty());

    let login = http()
        .post(format!("{}/auth/login", app.base))
        .json(&serde_json::json!({
            "email": email,
            "password": "password-123"
        }))
        .send()
        .await
        .expect("login");
    assert_eq!(login.status(), reqwest::StatusCode::OK);
    let body: serde_json::Value = login.json().await.expect("json");
    assert!(body["token"].as_str().is_some());

    let wrong = http()
        .post(format!("{}/auth/login", app.base))
        .json(&serde_json::json!({
            "email": email,
            "password": "not-the-password"
        }))
        .send()
        .await
        .expect("login");
    assert_eq!(wrong.status(), reqwest::StatusCode::UNAUTHORIZED);

    let duplicate = http()
        .post(format!("{}/auth/register", app.base))
        .json(&serde_json::json!({
            "email": email,
            "password": "password-123"
        }))
        .send()
        .await
        .expect("register");
    assert_eq!(duplicate.status(), reqwest::StatusCode::CONFLICT);

    let unauthenticated = http()
        .post(format!("{}/links", app.base))
        .json(&serde_json::json!({"url": "https://example.com"}))
        .send()
        .await
        .expect("create");
    assert_eq!(unauthenticated.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_redirect_stats_list_and_delete() {
    let app = Running::start(test_config()).await;
    let token = register(&app.base, &unique_email(), "password-123").await;
    let other = register(&app.base, &unique_email(), "password-123").await;
    let code = short_code("doc");

    let created = http()
        .post(format!("{}/links", app.base))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "url": "https://example.com/docs",
            "custom_code": code
        }))
        .send()
        .await
        .expect("create");
    assert_eq!(created.status(), reqwest::StatusCode::CREATED);
    let link: serde_json::Value = created.json().await.expect("json");
    assert_eq!(link["code"], code);
    assert_eq!(link["original_url"], "https://example.com/docs");

    let redirect = http()
        .get(format!("{}/{}", app.base, code))
        .header("referer", "https://news.example/article")
        .header("x-country-code", "fr")
        .send()
        .await
        .expect("redirect");
    assert_eq!(redirect.status(), reqwest::StatusCode::FOUND);
    assert_eq!(
        redirect
            .headers()
            .get("location")
            .and_then(|value| value.to_str().ok()),
        Some("https://example.com/docs")
    );
    assert_eq!(
        redirect
            .headers()
            .get("cache-control")
            .and_then(|value| value.to_str().ok()),
        Some("no-store")
    );

    let mut stats = None;
    for _ in 0..40 {
        let response = http()
            .get(format!("{}/links/{code}/stats", app.base))
            .bearer_auth(&token)
            .send()
            .await
            .expect("stats");
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let body: serde_json::Value = response.json().await.expect("json");
        if body["total_clicks"].as_i64() == Some(1) {
            stats = Some(body);
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let stats = stats.expect("le clic doit être visible après le flush");
    assert_eq!(stats["clicks_by_country"][0]["key"], "FR");
    assert_eq!(
        stats["clicks_by_referrer"][0]["key"],
        "https://news.example/article"
    );
    assert!(
        stats["clicks_by_day"]
            .as_array()
            .is_some_and(|days| !days.is_empty())
    );

    let forbidden = http()
        .get(format!("{}/links/{code}/stats", app.base))
        .bearer_auth(&other)
        .send()
        .await
        .expect("stats");
    assert_eq!(forbidden.status(), reqwest::StatusCode::FORBIDDEN);

    let list = http()
        .get(format!("{}/links", app.base))
        .bearer_auth(&token)
        .send()
        .await
        .expect("list");
    assert_eq!(list.status(), reqwest::StatusCode::OK);
    let list: serde_json::Value = list.json().await.expect("json");
    assert!(
        list.as_array()
            .expect("liste")
            .iter()
            .any(|item| item["code"] == code)
    );

    let deleted = http()
        .delete(format!("{}/links/{code}", app.base))
        .bearer_auth(&token)
        .send()
        .await
        .expect("delete");
    assert_eq!(deleted.status(), reqwest::StatusCode::NO_CONTENT);

    let missing = http()
        .get(format!("{}/{}", app.base, code))
        .send()
        .await
        .expect("redirect");
    assert_eq!(missing.status(), reqwest::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn expired_link_is_not_found() {
    let app = Running::start(test_config()).await;
    let token = register(&app.base, &unique_email(), "password-123").await;
    let code = short_code("exp");
    let created = http()
        .post(format!("{}/links", app.base))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "url": "https://example.com/expired",
            "custom_code": code
        }))
        .send()
        .await
        .expect("create");
    assert_eq!(created.status(), reqwest::StatusCode::CREATED);

    sqlx::query("UPDATE links SET expires_at = ? WHERE code = ?")
        .bind(chrono::Utc::now().naive_utc() - chrono::Duration::seconds(5))
        .bind(&code)
        .execute(&app.context.state.db)
        .await
        .expect("update");

    let response = http()
        .get(format!("{}/{}", app.base, code))
        .send()
        .await
        .expect("redirect");
    assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn unknown_code_and_invalid_url() {
    let app = Running::start(test_config()).await;
    let missing = http()
        .get(format!(
            "{}/missing{}",
            app.base,
            uuid::Uuid::now_v7().simple()
        ))
        .send()
        .await
        .expect("redirect");
    assert_eq!(missing.status(), reqwest::StatusCode::NOT_FOUND);

    let token = register(&app.base, &unique_email(), "password-123").await;
    let invalid = http()
        .post(format!("{}/links", app.base))
        .bearer_auth(&token)
        .json(&serde_json::json!({"url": "ftp://example.com"}))
        .send()
        .await
        .expect("create");
    assert_eq!(invalid.status(), reqwest::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn rate_limit_blocks_an_ip() {
    let mut config = test_config();
    config.rate_limit_ip_per_minute = 2;
    let app = Running::start(config).await;

    let first = http()
        .get(format!("{}/health", app.base))
        .send()
        .await
        .expect("health");
    let second = http()
        .get(format!("{}/health", app.base))
        .send()
        .await
        .expect("health");
    let third = http()
        .get(format!("{}/health", app.base))
        .send()
        .await
        .expect("health");
    assert_eq!(first.status(), reqwest::StatusCode::OK);
    assert_eq!(second.status(), reqwest::StatusCode::OK);
    assert_eq!(third.status(), reqwest::StatusCode::TOO_MANY_REQUESTS);
    assert!(third.headers().get("retry-after").is_some());
}
