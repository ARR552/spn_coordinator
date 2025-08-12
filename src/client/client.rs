use anyhow::Result;
use rpc_types::*;
use tonic::{Request, Response, Status, transport::{Channel, Endpoint, ClientTlsConfig}};
use prost::Message;
use std::time::Duration;
use ethers::{abi::token::LenientTokenizer, utils::keccak256};
use ethers::signers::{LocalWallet};
use std::str::FromStr;

/// Real gRPC client that makes actual gRPC calls
pub struct ProverNetworkClient {
    client: prover_network_client::ProverNetworkClient<Channel>,
}

impl ProverNetworkClient {
    pub async fn new(endpoint: String) -> Result<Self, Box<dyn std::error::Error>> {
        // Load the CA certificate to verify the server certificate
        println!("Setting up TLS client configuration with proper CA...");
        
        let ca_pem = std::fs::read("testing-cert/ca.pem")
            .map_err(|e| format!("Failed to read CA certificate: {}", e))?;
        
        println!("Loaded CA certificate, size: {} bytes", ca_pem.len());
        
        // Configure TLS with the proper CA certificate
        let tls_config = ClientTlsConfig::new()
            .ca_certificate(tonic::transport::Certificate::from_pem(&ca_pem))
            .domain_name("localhost");
        let tls_activated = false; // Set to true if TLS is enabled
        println!("Attempting to connect with TLS to: {}", endpoint);
        
        let mut endpoint = Endpoint::new(endpoint)
            .map_err(|e| format!("Invalid endpoint: {}", e))?
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(15))
            .keep_alive_while_idle(true)
            .http2_keep_alive_interval(Duration::from_secs(15))
            .keep_alive_timeout(Duration::from_secs(15))
            .tcp_keepalive(Some(Duration::from_secs(30)));
        if tls_activated {
            endpoint = endpoint.tls_config(tls_config).map_err(|e| format!("TLS config error: {}", e))?
        }

        let channel: Channel = endpoint.connect()
            .await
            .map_err(|e| format!("Connection failed: {}", e))?;
            
        println!("TLS connection established successfully");
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

async fn sign_body(wallet: &LocalWallet, encoded_message: Vec<u8>) -> anyhow::Result<Vec<u8>> {
    // 2. EIP-191 prefix
    let prefix = format!("\x19Ethereum Signed Message:\n{}", encoded_message.len());
    let mut prefixed = prefix.into_bytes();
    prefixed.extend_from_slice(&encoded_message);

    // 3. Hash
    let hash = keccak256(prefixed);

    // 4. Sign
    let sig = wallet.sign_hash(hash.into())?;

    // Return as raw r||s||v bytes
    Ok(sig.to_vec())
}

/// Create a sample RequestProofRequest with correct field names
pub async fn create_sample_request() -> anyhow::Result<RequestProofRequest> {
    let wallet = LocalWallet::from_str("0xe5d76acbffb5be6d87002e2cd5622b6dfe715f73ac60c613f14ba2d3f735c20b")?;
    let b = RequestProofRequestBody {
        nonce: 1,
        vk_hash: hex::decode("00199e4c35364a8ed49c9fac0f0940aa555ce166aafc1ccb24f57d245f9c962c").unwrap(),
        version: "sp1-v5.0.0".to_string(),
        mode: ProofMode::Plonk as i32,
        strategy: FulfillmentStrategy::Reserved as i32,
        stdin_uri: "s3://spn-artifacts-production3/stdins/artifact_01k1zkd4ntf21bcqgp5539zb9a".to_string(),
        deadline: 1754481839, // Mock timestamp
        cycle_limit: 55869569,
        gas_limit: 1000000000,
        min_auction_period: 60,
        whitelist: vec![],
        domain: b"mock_domain".to_vec(),
        auctioneer: hex::decode("d8d77442e6dc01d11fd7dcfa230198253f5f76ee").unwrap(),
        executor: b"mock_executor".to_vec(),
        verifier: b"mock_verifier".to_vec(),
        public_values_hash: Some(hex::decode("0068e255db4186f38c1da5d71ad3edafc0b4373d8131b47626f6e2d5a8e8fe98").unwrap()),
        base_fee: "1000000000000000000".to_string(), // 1 ETH in wei
        max_price_per_pgu: "1000000000".to_string(), // 1 Gwei
        variant: TransactionVariant::RequestVariant as i32,
        treasury: b"mock_treasury".to_vec(),
    };
    let mut buf = Vec::new();
    b.encode(&mut buf).expect("prost encode failed");
    let signature = sign_body(&wallet, buf).await?;

    // let sig: Vec<u8> = b.sign(&signer).into();
    Ok(RequestProofRequest {
        format: MessageFormat::Json as i32,
        signature: signature,
        body: Some(b),
    })
}

/// Client function that connects to the server
pub async fn run_client() -> Result<()> {
    println!("\n=== Starting Client ===");
    
    // Wait a bit for server to start
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    
    let mut client = ProverNetworkClient::new("http://127.0.0.1:50051".to_string()).await
        .map_err(|e| {
            eprintln!("Detailed client creation error: {:?}", e);
            anyhow::anyhow!("Failed to create client: {}", e)
        })?;
    
    println!("\n--- Client Request ---");
    
    // Create a different sample request for each iteration
    let request = create_sample_request().await?;
    
    println!("Client sending proof request ");
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
    
    println!("\n=== Client Finished ===");
    Ok(())
}
