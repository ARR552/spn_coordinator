use anyhow::Result;
use rpc_types::*;
use std::time::Duration;
use tonic::{Request, Response, Status, transport::{Channel, Endpoint}};

/// Real gRPC client that makes actual gRPC calls
pub struct ProverNetworkClient {
    client: prover_network_client::ProverNetworkClient<Channel>,
}

impl ProverNetworkClient {
    pub async fn new(url: String) -> Result<Self, Box<dyn std::error::Error>> {
        let channel: Channel = Endpoint::new(url)?
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(15))
            .keep_alive_while_idle(true)
            .http2_keep_alive_interval(Duration::from_secs(15))
            .keep_alive_timeout(Duration::from_secs(15))
            .tcp_keepalive(Some(Duration::from_secs(30)))
            .connect()
            .await?;

        let client = prover_network_client::ProverNetworkClient::new(channel);
        Ok(Self { client })
    }
    
    pub async fn request_proof(
        &mut self,
        request: RequestProofRequest,
    ) -> Result<Response<RequestProofResponse>, Status> {
        println!("Client sending real gRPC request to server");
        self.client.request_proof(Request::new(request)).await
    }
    
    pub async fn get_proof_request_status(
        &mut self,
        request: GetProofRequestStatusRequest,
    ) -> Result<Response<GetProofRequestStatusResponse>, Status> {
        println!("Client requesting real gRPC status from server for: {:?}", hex::encode(&request.request_id));
        self.client.get_proof_request_status(Request::new(request)).await
    }
    pub async fn get_proof_request_details(
        &mut self,
        request: GetProofRequestDetailsRequest,
    ) -> Result<Response<GetProofRequestDetailsResponse>, Status> {
        println!("Client requesting proof request details for: {:?}", hex::encode(&request.request_id));
        self.client.get_proof_request_details(Request::new(request)).await
    }
}

/// Client function that connects to the server
pub async fn run_client() -> Result<()> {
    println!("\n=== Starting Client ===");
    
    // Wait a bit for server to start
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    
    let mut client = ProverNetworkClient::new("https://rpc.production.succinct.xyz".to_string()).await
        .map_err(|e| anyhow::anyhow!("Failed to create client: {}", e))?;
    
    // Make multiple requests to demonstrate client-server interaction
    println!("\n--- Client Request ---");        
        
    // println!("Client sending proof request #{}", i);
    // let response = client.request_proof(request).await?;
    // let response_inner = response.into_inner();

    let request = GetProofRequestDetailsRequest {
        request_id: hex::decode("aa451513b7d68dc05f8fdf0be30fe95496129961e2c0960796a95ec72d6980a2").unwrap(),
    };
    let response = client.get_proof_request_details(request).await?;
    let response_inner = response.into_inner();
    
    println!("Client received Full response:");
    print_proof_request_details(&response_inner);
    
    // if let Some(body) = &response_inner.body {
    //     println!("Request ID: {}", hex::encode(&body.request_id));
        
    //     // Check status
    //     let status_request = GetProofRequestStatusRequest {
    //         request_id: body.request_id.clone(),
    //     };
        
    //     let status_response = client.get_proof_request_status(status_request).await?;
    //     let status_inner = status_response.into_inner();
        
    //     println!("Status check: {:?}", status_inner);
    //     println!("  Fulfillment: {:?}", FulfillmentStatus::try_from(status_inner.fulfillment_status).unwrap_or(FulfillmentStatus::UnspecifiedFulfillmentStatus));
    //     println!("  Execution: {:?}", ExecutionStatus::try_from(status_inner.execution_status).unwrap_or(ExecutionStatus::UnspecifiedExecutionStatus));
    //     if let Some(proof_uri) = &status_inner.proof_uri {
    //         println!("  Proof URI: {}", proof_uri);
    //     }
    // }
    
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

/// Format Unix timestamp to human-readable date
fn format_timestamp(timestamp: u64) -> String {
    use std::time::{SystemTime, UNIX_EPOCH, Duration};
    
    let datetime = UNIX_EPOCH + Duration::from_secs(timestamp);
    match datetime.duration_since(SystemTime::now()) {
        Ok(future) => format!("in {} seconds", future.as_secs()),
        Err(_) => {
            let past = SystemTime::now().duration_since(datetime).unwrap();
            format!("{} seconds ago", past.as_secs())
        }
    }
}
