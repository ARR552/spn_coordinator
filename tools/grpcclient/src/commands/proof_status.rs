use anyhow::Result;
use rpc_types::*;
use crate::client::ProverNetworkClient;
use crate::utils::format_timestamp;

pub async fn run_proof_request_status(url: String, request_id: String) -> Result<()> {
    println!("\n=== run_proof_request_status ===");
    
    let mut client = ProverNetworkClient::new(url).await
        .map_err(|e| anyhow::anyhow!("Failed to create client: {}", e))?;
    
    println!("\n--- Client Request ---");

    let request = GetProofRequestStatusRequest {
        request_id: hex::decode(&request_id)
            .map_err(|e| anyhow::anyhow!("Invalid request_id hex: {}", e))?,
    };

    let response = client.get_proof_request_status(request).await?;
    let response_inner = response.into_inner();
    
    println!("Client received status response:");
    print_proof_request_status(&response_inner);
    
    println!("\n=== Client Finished ===");
    Ok(())
}

/// Pretty print the proof request status with hex-encoded byte arrays
fn print_proof_request_status(response: &GetProofRequestStatusResponse) {
    println!("GetProofRequestStatusResponse {{");
    println!("  fulfillment_status: {:?}", FulfillmentStatus::try_from(response.fulfillment_status).unwrap_or(FulfillmentStatus::UnspecifiedFulfillmentStatus));
    println!("  execution_status: {:?}", ExecutionStatus::try_from(response.execution_status).unwrap_or(ExecutionStatus::UnspecifiedExecutionStatus));
    println!("  request_tx_hash: \"{}\"", hex::encode(&response.request_tx_hash));
    println!("  deadline: {} ({})", response.deadline, format_timestamp(response.deadline));
    
    if let Some(fulfill_tx_hash) = &response.fulfill_tx_hash {
        println!("  fulfill_tx_hash: Some(\"{}\")", hex::encode(fulfill_tx_hash));
    } else {
        println!("  fulfill_tx_hash: None");
    }
    
    if let Some(proof_uri) = &response.proof_uri {
        println!("  proof_uri: Some(\"{}\")", proof_uri);
    } else {
        println!("  proof_uri: None");
    }
    
    if let Some(public_values_hash) = &response.public_values_hash {
        println!("  public_values_hash: Some(\"{}\")", hex::encode(public_values_hash));
    } else {
        println!("  public_values_hash: None");
    }
    
    if let Some(proof_public_uri) = &response.proof_public_uri {
        println!("  proof_public_uri: Some(\"{}\")", proof_public_uri);
    } else {
        println!("  proof_public_uri: None");
    }
    
    println!("}}");
}
