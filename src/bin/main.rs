use anyhow::Result;
use spn_coordinator::client::run_client;
use spn_coordinator::server::run_server;
use tokio::sync::mpsc;
use tokio::signal;

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
            println!("\nReceived Ctrl+C signal");
        },
        _ = terminate => {
            println!("\nReceived terminate signal");
        },
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize crypto provider before any TLS operations
    init_crypto_provider();
    
    println!("ProverNetwork gRPC - Server/Client Architecture");
    println!("===================================================");
    
    // Create shutdown channel
    let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(1);
    
    // Spawn server task that runs in background
    let server_handle = tokio::spawn(async move {
        if let Err(e) = run_server(shutdown_rx).await {
            eprintln!("Server error: {}", e);
        }
    });
    
    // Spawn client task
    let client_handle = tokio::spawn(async move {
        if let Err(e) = run_client().await {
            eprintln!("Client error: {}", e);
        }
    });
    
    // Spawn signal handler task
    let signal_handle = tokio::spawn(async move {
        shutdown_signal().await;
        println!("Sending shutdown signal to server...");
        let _ = shutdown_tx.send(()).await;
    });
    
    // Wait for client to finish
    let _ = client_handle.await;
    println!("\nClient completed. Server continues running in background...");
    println!("Press Ctrl+C to gracefully shutdown the server");
    
    // Wait for shutdown signal
    let _ = signal_handle.await;
    
    // Give the server a moment to process the shutdown signal
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    // Wait for server to finish gracefully
    let _ = server_handle.await;
    
    println!("\n=== Complete ===");
    println!("This demonstrates:");
    println!("1. Server running continuously in background thread");
    println!("2. Client making multiple requests and then finishing");
    println!("3. Proper concurrent execution using tokio tasks");
    println!("4. Server continuing to process after client disconnects");
    println!("5. Graceful shutdown handling with signals (Ctrl+C, SIGTERM)");
    
    Ok(())
}
