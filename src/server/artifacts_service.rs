use anyhow::Result;
use rpc_types::*;
use std::collections::HashMap;
use tokio::sync::Mutex;
use tonic::{Request, Response, Status};
use eyre;
use rand::random;

/// Real gRPC service implementation for ArtifactStore
#[derive(Debug, Default)]
pub struct ArtifactStoreServiceImpl {
    /// TODO Store artifacts in memory (in real implementation this would be a database or S3)  
    artifacts: Mutex<HashMap<String, (ArtifactType, String)>>, // artifact_uri -> (type, presigned_url)
}

#[tonic::async_trait]
impl artifact_store_server::ArtifactStore for ArtifactStoreServiceImpl {
    /// Creates an artifact that can be used for proof requests.
    async fn create_artifact(
        &self,
        request: Request<CreateArtifactRequest>,
    ) -> Result<Response<CreateArtifactResponse>, Status> {
        let req = request.into_inner();
        println!("ARTIFACT: Server received create_artifact request with signature: {:?}", hex::encode(&req.signature));
        
        // Validate the artifact type
        let artifact_type = ArtifactType::try_from(req.artifact_type)
            .map_err(|_| Status::invalid_argument("Invalid artifact type"))?;
        
        // TODO: Verify signature for authentication
        // For now, we'll skip signature verification as it would require the signed message format
        
        // Generate unique artifact URI and presigned URL
        let artifact_id = generate_artifact_id();
        let artifact_uri = generate_artifact_uri(&artifact_id);
        let presigned_url = generate_presigned_url(&artifact_id);
        
        println!("ARTIFACT: Generated artifact URI: {}", artifact_uri);
        println!("ARTIFACT: Generated presigned URL: {}", presigned_url);
        
        // Store the artifact metadata
        self.artifacts.lock().await.insert(
            artifact_uri.clone(), 
            (artifact_type, presigned_url.clone())
        );
        
        let response = CreateArtifactResponse {
            artifact_uri: artifact_uri.clone(),
            artifact_presigned_url: presigned_url,
        };
        
        println!("ARTIFACT: Successfully created artifact: {}", artifact_uri);
        Ok(Response::new(response))
    }
}

/// Generate a unique artifact identifier
fn generate_artifact_id() -> String {
    let id_bytes = random::<[u8; 16]>();
    hex::encode(id_bytes)
}

/// Generate an artifact URI based on the ID
fn generate_artifact_uri(artifact_id: &str) -> String {    
    format!("s3://spn-artifacts/{}", artifact_id)
}

/// Generate a presigned URL for artifact upload
fn generate_presigned_url(artifact_id: &str) -> String {
    // Generate a URL pointing to our HTTP server
    // The client will use this URL to PUT the artifact data
    format!("http://spn-coordinator-001:8082/artifacts/{}", artifact_id)
}

/// Verify signature for artifact creation (placeholder implementation)
fn _verify_artifact_signature(signature: &[u8]) -> Result<Vec<u8>, eyre::Error> {
    // TODO: Implement proper signature verification
    // This would require:
    // 1. Define the message format for artifact creation
    // 2. Implement the same signing/verification logic as in prover_network_service.rs
    // 3. Recover the signer address from the signature
    println!("ARTIFACT: Verifying artifact signature: {:?}", hex::encode(signature));
    // For now, return a mock address
    let mock_address = vec![0x42; 20]; // Mock 20-byte address
    Ok(mock_address)
}
