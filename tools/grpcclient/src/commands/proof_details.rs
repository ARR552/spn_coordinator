use anyhow::Result;
use rpc_types::*;
use crate::client::ProverNetworkClient;
use crate::utils::format_timestamp;

pub async fn run_proof_request_details(url: String, request_id: String) -> Result<()> {
    println!("\n=== run_proof_request_details ===");
    
    let mut client = ProverNetworkClient::new(url).await
        .map_err(|e| anyhow::anyhow!("Failed to create client: {}", e))?;
    
    // Make multiple requests to demonstrate client-server interaction
    println!("\n--- Client Request ---");

    let request = GetProofRequestDetailsRequest {
        request_id: hex::decode(&request_id)
            .map_err(|e| anyhow::anyhow!("Invalid request_id hex: {}", e))?,
    };

    // let request = GetProofRequestDetailsRequest {
    //     request_id: hex::decode("4e94a6a152d166b9c26faf27e406ead95b60aee02da50294e10a46131fbb9f5f").unwrap(), //aa451513b7d68dc05f8fdf0be30fe95496129961e2c0960796a95ec72d6980a2
    // };
    let response = client.get_proof_request_details(request).await?;
    let response_inner = response.into_inner();
    
    println!("Client received Full response:");
    print_proof_request_details(&response_inner);
    
    // Wait between requests
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    
    println!("\n=== Client Finished ===");
    Ok(())
}

/// Pretty print the proof request details with hex-encoded byte arrays
fn print_proof_request_details(response: &GetProofRequestDetailsResponse) {
    println!("GetProofRequestDetailsResponse {{");
    
    if let Some(request) = &response.request {
        println!("  request: Some(ProofRequest {{");
        println!("    request_id: \"{}\"", hex::encode(&request.request_id));
        println!("    vk_hash: \"{}\"", hex::encode(&request.vk_hash));
        println!("    version: \"{}\"", request.version);
        println!("    mode: {:?}", ProofMode::try_from(request.mode).unwrap_or(ProofMode::UnspecifiedProofMode));
        println!("    strategy: {:?}", FulfillmentStrategy::try_from(request.strategy).unwrap_or(FulfillmentStrategy::UnspecifiedFulfillmentStrategy));
        println!("    program_uri: \"{}\"", request.program_uri);
        println!("    stdin_uri: \"{}\"", request.stdin_uri);
        println!("    deadline: {} ({})", request.deadline, format_timestamp(request.deadline));
        println!("    cycle_limit: {}", request.cycle_limit);
        
        if let Some(gas_price) = &request.gas_price {
            println!("    gas_price: Some(\"{}\")", gas_price);
        } else {
            println!("    gas_price: None");
        }
        
        println!("    fulfillment_status: {:?}", FulfillmentStatus::try_from(request.fulfillment_status).unwrap_or(FulfillmentStatus::UnspecifiedFulfillmentStatus));
        println!("    execution_status: {:?}", ExecutionStatus::try_from(request.execution_status).unwrap_or(ExecutionStatus::UnspecifiedExecutionStatus));
        println!("    requester: \"{}\"", hex::encode(&request.requester));
        
        if let Some(fulfiller) = &request.fulfiller {
            println!("    fulfiller: Some(\"{}\")", hex::encode(fulfiller));
        } else {
            println!("    fulfiller: None");
        }
        
        if let Some(program_name) = &request.program_name {
            println!("    program_name: Some(\"{}\")", program_name);
        } else {
            println!("    program_name: None");
        }
        
        if let Some(requester_name) = &request.requester_name {
            println!("    requester_name: Some(\"{}\")", requester_name);
        } else {
            println!("    requester_name: None");
        }
        
        if let Some(fulfiller_name) = &request.fulfiller_name {
            println!("    fulfiller_name: Some(\"{}\")", fulfiller_name);
        } else {
            println!("    fulfiller_name: None");
        }
        
        println!("    created_at: {} ({})", request.created_at, format_timestamp(request.created_at));
        println!("    updated_at: {} ({})", request.updated_at, format_timestamp(request.updated_at));
        
        if let Some(fulfilled_at) = request.fulfilled_at {
            println!("    fulfilled_at: Some({}) ({})", fulfilled_at, format_timestamp(fulfilled_at));
        } else {
            println!("    fulfilled_at: None");
        }
        
        println!("    tx_hash: \"{}\"", hex::encode(&request.tx_hash));
        
        if let Some(cycles) = request.cycles {
            println!("    cycles: Some({})", cycles);
        } else {
            println!("    cycles: None");
        }
        
        if let Some(public_values_hash) = &request.public_values_hash {
            println!("    public_values_hash: Some(\"{}\")", hex::encode(public_values_hash));
        } else {
            println!("    public_values_hash: None");
        }
        
        println!("    gas_limit: {}", request.gas_limit);
        
        if let Some(gas_used) = request.gas_used {
            println!("    gas_used: Some({})", gas_used);
        } else {
            println!("    gas_used: None");
        }
        
        println!("    program_public_uri: \"{}\"", request.program_public_uri);
        println!("    stdin_public_uri: \"{}\"", request.stdin_public_uri);
        println!("    min_auction_period: {}", request.min_auction_period);
        println!("    whitelist: {:?}", request.whitelist);
        
        println!("  }})");
    } else {
        println!("  request: None");
    }
    
    println!("}}");
}
