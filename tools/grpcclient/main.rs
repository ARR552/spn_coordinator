use anyhow::Result;
use grpc_client_tool::client::run_client;
use logger;

#[tokio::main]
async fn main() -> Result<()> {
    logger::init();
    if let Err(e) = run_client().await {
        tracing::error!("Error: {}", e);
        std::process::exit(1);
    }
    Ok(())
}
