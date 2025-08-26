use anyhow::Result;
use spn_coordinator::server::HttpServer;
use logger;

#[tokio::main]
async fn main() -> Result<()> {
    logger::init();
    tracing::info!("Starting HTTP server only...");
    
    let http_server = HttpServer::new(8082); // Use port 8082 to avoid conflict
    if let Err(e) = http_server.start().await {
        tracing::error!("HTTP server error: {}", e);
        return Err(anyhow::anyhow!("HTTP server failed: {}", e));
    }
    
    Ok(())
}
