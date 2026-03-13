use std::net::SocketAddr;

use tokio::net::TcpListener;
use url_shortener::{AppContext, Config, router};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "url_shortener=info,tower_http=info".into()),
        )
        .init();

    let config = Config::from_env()?;
    let bind_addr: SocketAddr = config.bind_addr.parse()?;
    let context = AppContext::connect(config).await?;
    let listener = TcpListener::bind(bind_addr).await?;
    tracing::info!(%bind_addr, "serveur démarré");

    let state = context.state.clone();
    axum::serve(
        listener,
        router(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    tracing::info!("vidage des clics en attente");
    context.shutdown().await;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(err) = tokio::signal::ctrl_c().await {
            tracing::error!(error = %err, "impossible d'écouter Ctrl+C");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(err) => {
                tracing::error!(error = %err, "impossible d'écouter SIGTERM");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("signal d'arrêt reçu");
}
