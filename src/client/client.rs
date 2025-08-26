use anyhow::Result;
use rpc_types::*;
use tonic::{Request, Response, Status, transport::{Channel, Endpoint, ClientTlsConfig}};
use prost::Message;
use std::time::Duration;
use ethers::{utils::keccak256};
use ethers::signers::{LocalWallet};
use std::str::FromStr;

/// The zkvm ELF binaries.
pub const AGGREGATION_ELF: &[u8] = include_bytes!("./elf/aggregation-elf");
pub const RANGE_ELF_BUMP: &[u8] = include_bytes!("./elf/range-elf-bump");
pub const RANGE_ELF_EMBEDDED: &[u8] = include_bytes!("./elf/range-elf-embedded");
pub const CELESTIA_RANGE_ELF_EMBEDDED: &[u8] =
    include_bytes!("./elf/celestia-range-elf-embedded");
// TODO: Update to EigenDA Range ELF Embedded
pub const EIGENDA_RANGE_ELF_EMBEDDED: &[u8] = include_bytes!("./elf/range-elf-embedded");


/// Real gRPC client that makes actual gRPC calls
pub struct ProverNetworkClient {
    client: prover_network_client::ProverNetworkClient<Channel>,
}

/// Artifact service client for creating and managing artifacts
pub struct ArtifactServiceClient {
    client: artifact_store_client::ArtifactStoreClient<Channel>,
}

impl ArtifactServiceClient {
    pub async fn new(endpoint: String) -> Result<Self, Box<dyn std::error::Error>> {
        let tls_activated = false; // Set to true if TLS is enabled
        
        let mut endpoint = Endpoint::new(endpoint)
            .map_err(|e| format!("Invalid endpoint: {}", e))?
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(15))
            .keep_alive_while_idle(true)
            .http2_keep_alive_interval(Duration::from_secs(15))
            .keep_alive_timeout(Duration::from_secs(15))
            .tcp_keepalive(Some(Duration::from_secs(30)));
        if tls_activated {
            // Load the CA certificate to verify the server certificate
            tracing::info!("Setting up TLS client configuration for artifact service...");
            
            let ca_pem = std::fs::read("testing-cert/ca.pem")
                .map_err(|e| format!("Failed to read CA certificate: {}", e))?;
            
            tracing::debug!("Loaded CA certificate for artifact service, size: {} bytes", ca_pem.len());
            
            // Configure TLS with the proper CA certificate
            let tls_config = ClientTlsConfig::new()
                .ca_certificate(tonic::transport::Certificate::from_pem(&ca_pem))
                .domain_name("localhost");

            endpoint = endpoint.tls_config(tls_config).map_err(|e| format!("TLS config error: {}", e))?
        }

        let channel: Channel = endpoint.connect()
            .await
            .map_err(|e| format!("Connection failed: {}", e))?;
            
        let client = artifact_store_client::ArtifactStoreClient::new(channel);
        Ok(Self { client })
    }
    
    pub async fn create_artifact(
        &mut self,
        request: CreateArtifactRequest,
    ) -> Result<Response<CreateArtifactResponse>, Status> {
        self.client.create_artifact(Request::new(request)).await
    }
}

impl ProverNetworkClient {
    pub async fn new(endpoint: String) -> Result<Self, Box<dyn std::error::Error>> {
        let tls_activated = false; // Set to true if TLS is enabled
        
        let mut endpoint = Endpoint::new(endpoint)
            .map_err(|e| format!("Invalid endpoint: {}", e))?
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(15))
            .keep_alive_while_idle(true)
            .http2_keep_alive_interval(Duration::from_secs(15))
            .keep_alive_timeout(Duration::from_secs(15))
            .tcp_keepalive(Some(Duration::from_secs(30)));
        if tls_activated {
            // Load the CA certificate to verify the server certificate
            tracing::info!("Setting up TLS client configuration with proper CA...");
            
            let ca_pem = std::fs::read("testing-cert/ca.pem")
                .map_err(|e| format!("Failed to read CA certificate: {}", e))?;
            
            tracing::debug!("Loaded CA certificate, size: {} bytes", ca_pem.len());
            
            // Configure TLS with the proper CA certificate
            let tls_config = ClientTlsConfig::new()
                .ca_certificate(tonic::transport::Certificate::from_pem(&ca_pem))
                .domain_name("localhost");

            endpoint = endpoint.tls_config(tls_config).map_err(|e| format!("TLS config error: {}", e))?
        }

        let channel: Channel = endpoint.connect()
            .await
            .map_err(|e| format!("Connection failed: {}", e))?;
            
        let client = prover_network_client::ProverNetworkClient::new(channel);
        Ok(Self { client })
    }
    
    pub async fn request_proof(
        &mut self,
        request: RequestProofRequest,
    ) -> Result<Response<RequestProofResponse>, Status> {
        self.client.request_proof(Request::new(request)).await
    }
    
    pub async fn get_proof_request_status(
        &mut self,
        request: GetProofRequestStatusRequest,
    ) -> Result<Response<GetProofRequestStatusResponse>, Status> {
        self.client.get_proof_request_status(Request::new(request)).await
    }

    pub async fn create_program(
        &mut self,
        request: CreateProgramRequest,
    ) -> Result<Response<CreateProgramResponse>, Status> {
        self.client.create_program(Request::new(request)).await
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

pub async fn create_program_request(program_uri: String) -> anyhow::Result<CreateProgramRequest> {
    let program = rpc_types::CreateProgramRequestBody {
        vk_hash: hex::decode("005d763c1b4e00563d156f9ba8cc60561014267a5d3f5f16e2b8a47fa9dfe173").unwrap_or_default(),
        vk: hex::decode("18c19a61c29c213edfea9e0e5f7b35610f968f43282c5002be4fd123980b3a4644a92d00fecded6ac7efd272fca32d3f487d864ef12bf638be069326153b79650edd32370c739032ac70962f7b08ef1376627c701343d63742584c2c0200000000000000070000000000000050726f6772616d1400000000000000010000000e0000000000000000001000000000000400000000000000427974651000000000000000010000000b0000000000000000000100000000000200000000000000070000000000000050726f6772616d00000000000000000400000000000000427974650100000000000000").unwrap_or_default(),
        program_uri: program_uri,
        nonce: 0,
    };

    let mut buf = Vec::new();
    let wallet = LocalWallet::from_str("0xe5d76acbffb5be6d87002e2cd5622b6dfe715f73ac60c613f14ba2d3f735c20b")?;
    program.encode(&mut buf).expect("prost encode failed");
    let signature = sign_body(&wallet, buf).await?;
    let request = rpc_types::CreateProgramRequest {
        format: MessageFormat::Json as i32,
        signature: signature,
        body: Some(program),
    };
    
    return Ok(request);
}
pub async fn create_artifact_request(artifact_type: ArtifactType) -> anyhow::Result<CreateArtifactRequest> {
    let request = CreateArtifactRequest {
        artifact_type: artifact_type as i32,
        ..Default::default()
    };
    
    Ok(request)
}

/// Client function that connects to the server
pub async fn run_client() -> Result<()> {
    tracing::info!("=== Starting Client ===");
    
    // Wait a bit for server to start
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    
    let mut prover_network_client = ProverNetworkClient::new("http://127.0.0.1:50051".to_string()).await
        .map_err(|e| {
            tracing::error!("Detailed prover_network_client creation error: {:?}", e);
            anyhow::anyhow!("Failed to create prover_network_client: {}", e)
        })?;
    
    // Create artifact service client
    let mut artifact_client = ArtifactServiceClient::new("http://127.0.0.1:50051".to_string()).await
        .map_err(|e| {
            tracing::error!("Detailed artifact_client creation error: {:?}", e);
            anyhow::anyhow!("Failed to create artifact_client: {}", e)
        })?;
    
    let artifact_type = ArtifactType::Program;
    let artifact_request = create_artifact_request(artifact_type).await?;
    
    let response_inner = match artifact_client.create_artifact(artifact_request).await {
        Ok(response) => {
            let response_inner = response.into_inner();
            tracing::info!("✓ artifact created successfully!");
            tracing::info!("  Artifact URI: {}", response_inner.artifact_uri);
            tracing::info!("  Presigned URL: {}", response_inner.artifact_presigned_url);
            response_inner
        },
        Err(e) => {
            tracing::error!("✗ Failed to create artifact: {}", e);
            return Err(anyhow::anyhow!("Failed to create artifact: {}", e));
        }
    };
    
    // Upload the artifact using the presigned URL
    let artifact_bytes = AGGREGATION_ELF;
    tracing::info!("Uploading artifact ({} bytes) to presigned URL...", artifact_bytes.len());

    let put_url = response_inner.artifact_presigned_url.clone().replace("spn-coordinator-001", "localhost");
    let client = reqwest::Client::new();
    let upload_response = client
        .put(put_url.clone())
        .header("Content-Type", "application/binary")
        .body(bincode::serialize(artifact_bytes)?)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to upload artifact: {}", e))?;

    if upload_response.status().is_success() {
        tracing::info!("✓ Artifact uploaded successfully!");
    } else {
        tracing::error!("✗ Failed to upload artifact. Status: {}", upload_response.status());
        tracing::error!("Response: {:?}", upload_response.text().await);
    }

    // Create a request
    let request = create_program_request(response_inner.artifact_presigned_url.clone()).await?;
    
    tracing::info!("Client sending proof request ");
    // let response = client.request_proof(request).await?;
    let response = prover_network_client.create_program(request).await?;
    let response_inner = response.into_inner();
    
    tracing::info!("Client create program response: TX Hash = {}", hex::encode(&response_inner.tx_hash));
    
    // Wait between requests
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    
    Ok(())
}
