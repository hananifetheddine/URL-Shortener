use std::time::Duration;

use sqlx::MySqlPool;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::models::ClickEvent;

pub fn spawn_worker(
    pool: MySqlPool,
    mut receiver: mpsc::Receiver<ClickEvent>,
    batch_size: usize,
    flush_interval: Duration,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut batch = Vec::with_capacity(batch_size);
        let mut ticker = tokio::time::interval(flush_interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                biased;
                incoming = receiver.recv() => {
                    match incoming {
                        Some(event) => {
                            batch.push(event);
                            if batch.len() >= batch_size {
                                flush(&pool, &mut batch).await;
                            }
                        }
                        None => {
                            flush(&pool, &mut batch).await;
                            tracing::info!("worker de clics arrêté");
                            break;
                        }
                    }
                }
                _ = ticker.tick() => {
                    flush(&pool, &mut batch).await;
                }
            }
        }
    })
}

async fn flush(pool: &MySqlPool, batch: &mut Vec<ClickEvent>) {
    if batch.is_empty() {
        return;
    }

    for attempt in 1_u32..=3 {
        match crate::db::insert_click_batch(pool, batch).await {
            Ok(()) => {
                tracing::debug!(count = batch.len(), "clics enregistrés");
                batch.clear();
                return;
            }
            Err(err) => {
                tracing::error!(error = %err, attempt, "échec d'écriture des clics");
                tokio::time::sleep(Duration::from_millis(50 * u64::from(attempt))).await;
            }
        }
    }

    tracing::error!(
        lost = batch.len(),
        "clics abandonnés après plusieurs essais"
    );
    batch.clear();
}
