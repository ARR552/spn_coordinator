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
    
    println!("Client received Full response: {:?}", response_inner);
    
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
