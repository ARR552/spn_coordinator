use anyhow::Result;
use rpc_types::*;
use tonic::{Request, Response, Status, transport::{Channel, Endpoint}};

/// Real gRPC client that makes actual gRPC calls
pub struct ProverNetworkClient {
    client: prover_network_client::ProverNetworkClient<Channel>,
}

impl ProverNetworkClient {
    pub async fn new(endpoint: String) -> Result<Self, Box<dyn std::error::Error>> {
        let channel = Endpoint::from_shared(endpoint)?.connect().await?;
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
}

/// Create a sample RequestProofRequest with correct field names
pub fn create_sample_request() -> RequestProofRequest {
    RequestProofRequest {
        format: MessageFormat::Json as i32,
        signature: b"mock_signature".to_vec(),
        body: Some(RequestProofRequestBody {
            nonce: 1,
            vk_hash: b"mock_vk_hash".to_vec(),
            version: "1.0.0".to_string(),
            mode: ProofMode::Core as i32,
            strategy: FulfillmentStrategy::Hosted as i32,
            stdin_uri: "https://example.com/stdin".to_string(),
            deadline: 1234567890, // Mock timestamp
            cycle_limit: 1000000,
            gas_limit: 500000,
            min_auction_period: 0,
            whitelist: vec![],
            domain: b"mock_domain".to_vec(),
            auctioneer: b"mock_auctioneer".to_vec(),
            executor: b"mock_executor".to_vec(),
            verifier: b"mock_verifier".to_vec(),
            public_values_hash: Some(b"mock_public_values_hash".to_vec()),
            base_fee: "1000000000000000000".to_string(), // 1 ETH in wei
            max_price_per_pgu: "1000000000".to_string(), // 1 Gwei
            variant: TransactionVariant::RequestVariant as i32,
            treasury: b"mock_treasury".to_vec(),
        }),
    }
}

/// Client function that connects to the server
pub async fn run_client() -> Result<()> {
    println!("\n=== Starting Client ===");
    
    // Wait a bit for server to start
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    
    let mut client = ProverNetworkClient::new("http://127.0.0.1:50051".to_string()).await
        .map_err(|e| anyhow::anyhow!("Failed to create client: {}", e))?;
    
    // Make multiple requests to demonstrate client-server interaction
    for i in 1..=3 {
        println!("\n--- Client Request {} ---", i);
        
        // Create a different sample request for each iteration
        let mut request = create_sample_request();
        if let Some(body) = &mut request.body {
            body.mode = match i {
                1 => ProofMode::Core as i32,
                2 => ProofMode::Compressed as i32,
                _ => ProofMode::Plonk as i32,
            };
            body.strategy = match i {
                1 => FulfillmentStrategy::Hosted as i32,
                2 => FulfillmentStrategy::Auction as i32,
                _ => FulfillmentStrategy::Reserved as i32,
            };
            body.cycle_limit = 1000000 * i as u64;
            body.gas_limit = 500000 * i as u64;
            body.nonce = i as u64;
        }
        
        println!("Client sending proof request #{}", i);
        let response = client.request_proof(request).await?;
        let response_inner = response.into_inner();
        
        println!("Client received response: TX Hash = {}", hex::encode(&response_inner.tx_hash));
        
        if let Some(body) = &response_inner.body {
            println!("Request ID: {}", hex::encode(&body.request_id));
            
            // Check status
            let status_request = GetProofRequestStatusRequest {
                request_id: body.request_id.clone(),
            };
            
            let status_response = client.get_proof_request_status(status_request).await?;
            let status_inner = status_response.into_inner();
            
            println!("Status check: {:?}", status_inner);
            println!("  Fulfillment: {:?}", FulfillmentStatus::try_from(status_inner.fulfillment_status).unwrap_or(FulfillmentStatus::UnspecifiedFulfillmentStatus));
            println!("  Execution: {:?}", ExecutionStatus::try_from(status_inner.execution_status).unwrap_or(ExecutionStatus::UnspecifiedExecutionStatus));
            if let Some(proof_uri) = &status_inner.proof_uri {
                println!("  Proof URI: {}", proof_uri);
            }
        }
        
        // Wait between requests
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
    
    println!("\n=== Client Finished ===");
    Ok(())
}
