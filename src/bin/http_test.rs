use anyhow::Result;
use spn_coordinator::server::HttpServer;

#[tokio::main]
async fn main() -> Result<()> {
    println!("Starting HTTP server only...");
    
    let http_server = HttpServer::new(8082); // Use port 8082 to avoid conflict
    if let Err(e) = http_server.start().await {
        eprintln!("HTTP server error: {}", e);
        return Err(anyhow::anyhow!("HTTP server failed: {}", e));
    }
    
    Ok(())
}
