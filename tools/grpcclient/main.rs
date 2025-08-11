use anyhow::Result;
use grpc_client_tool::client::run_client;

#[tokio::main]
async fn main() -> Result<()> {
    if let Err(e) = run_client().await {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
    Ok(())
}
