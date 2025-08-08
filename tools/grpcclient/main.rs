use anyhow::Result;
use grpc_client_tool::client::run_client;

#[tokio::main]
async fn main() -> Result<()> {
    println!("ProverNetwork gRPC - Server/Client Architecture");
    println!("===================================================");
    
    // Spawn client task
    let client_handle = tokio::spawn(async move {
        if let Err(e) = run_client().await {
            eprintln!("Client error: {}", e);
        }
    });

    // Wait for client to finish
    let _ = client_handle.await;

    println!("Finish!! ...");
    Ok(())
}
