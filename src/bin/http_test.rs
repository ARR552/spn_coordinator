use anyhow::Result;
use spn_coordinator::server::HttpServer;
use spn_coordinator::config::ServerConfig;
use logger;

#[tokio::main]
async fn main() -> Result<()> {
    logger::init();
    tracing::info!("Starting HTTP server only...");
    let config = ServerConfig {
        db_path: "./db/test/".to_string(),
        http_addr: "0.0.0.0".to_string(),
        http_port: 8082,
        ..Default::default()
    };
    let http_server = HttpServer::new(config)?; // Use port 8082 to avoid conflict
    if let Err(e) = http_server.start().await {
        tracing::error!("HTTP server error: {}", e);
        return Err(anyhow::anyhow!("HTTP server failed: {}", e));
    }
    
    Ok(())
}
