use std::sync::Arc;

use redis::aio::ConnectionManager;
use sqlx::MySqlPool;
use sqlx::mysql::MySqlPoolOptions;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::clicks;
use crate::config::Config;
use crate::error::AppError;
use crate::models::ClickEvent;

#[derive(Clone)]
pub struct AppState {
    pub db: MySqlPool,
    pub redis: ConnectionManager,
    pub click_tx: mpsc::Sender<ClickEvent>,
    pub config: Arc<Config>,
}

pub struct AppContext {
    pub state: AppState,
    worker: JoinHandle<()>,
}

impl AppContext {
    pub async fn connect(config: Config) -> Result<Self, AppError> {
        let pool = MySqlPoolOptions::new()
            .max_connections(config.db_max_connections)
            .acquire_timeout(std::time::Duration::from_secs(5))
            .connect(&config.database_url)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;

        let client = redis::Client::open(config.redis_url.as_str())?;
        let redis = ConnectionManager::new(client).await?;

        let (click_tx, click_rx) = mpsc::channel(config.click_channel_capacity);
        let worker = clicks::spawn_worker(
            pool.clone(),
            click_rx,
            config.click_batch_size,
            config.click_flush_interval,
        );

        let state = AppState {
            db: pool,
            redis,
            click_tx,
            config: Arc::new(config),
        };
        Ok(Self { state, worker })
    }

    pub fn stop(&self) {
        self.worker.abort();
    }

    pub async fn shutdown(self) {
        let Self { state, worker } = self;
        drop(state);
        if let Err(err) = worker.await {
            tracing::error!(error = %err, "le worker de clics a paniqué");
        }
    }
}
