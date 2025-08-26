use anyhow::Result;
use spn_coordinator::client::run_client;
use spn_coordinator::server::run_server;
use tokio::sync::mpsc;
use tokio::signal;
use logger;

// Initialize rustls crypto provider
fn init_crypto_provider() {
    use rustls::crypto::ring::default_provider;
    let _ = rustls::crypto::CryptoProvider::install_default(default_provider());
}

/// Handle shutdown signals (SIGINT, SIGTERM)
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            tracing::info!("Received Ctrl+C signal");
        },
        _ = terminate => {
            tracing::info!("Received terminate signal");
        },
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logger
    logger::init();

    // Initialize crypto provider before any TLS operations
    init_crypto_provider();

    tracing::info!("ProverNetwork gRPC - Server/Client Architecture");
    tracing::info!("===================================================");
    
    // Create shutdown channel
    let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(1);
    
    // Spawn server task that runs in background
    let server_handle = tokio::spawn(async move {
        if let Err(e) = run_server(shutdown_rx).await {
            tracing::error!("Server error: {}", e);
        }
    });
    
    // Spawn client task
    let client_handle = tokio::spawn(async move {
        if let Err(e) = run_client().await {
            tracing::error!("Client error: {}", e);
        }
    });
    
    // Spawn signal handler task
    let signal_handle = tokio::spawn(async move {
        shutdown_signal().await;
        tracing::debug!("Sending shutdown signal to server...");
        let _ = shutdown_tx.send(()).await;
    });
    
    // Wait for client to finish
    let _ = client_handle.await;
    tracing::info!("Client completed. Server continues running in background...");
    tracing::info!("Press Ctrl+C to gracefully shutdown the server");
    
    // Wait for shutdown signal
    let _ = signal_handle.await;
    
    // Give the server a moment to process the shutdown signal
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    // Wait for server to finish gracefully
    let _ = server_handle.await;
    
    Ok(())
}
