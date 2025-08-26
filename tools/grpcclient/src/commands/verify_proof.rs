use anyhow::Result;
use bincode;
use sp1_sdk::{ProverClient, SP1ProofWithPublicValues, SP1VerifyingKey};

pub async fn run_verify_proof(
    proof_url: Option<String>, 
    proof_file: Option<String>, 
    vk_string: String, 
) -> Result<()> {
    sp1_sdk::utils::setup_logger();
    tracing::info!("=== Run verify_proof ===");
    
    // Load proof data from URL or file
    let proof_data = match (proof_url, proof_file) {
        (Some(url), None) => {
            tracing::info!("Loading proof from URL: {}", url);
            load_proof_from_url(url).await?
        }
        (None, Some(file_path)) => {
            tracing::info!("Loading proof from file: {}", file_path);
            load_proof_from_file(file_path).await?
        }
        (Some(_), Some(_)) => {
            return Err(anyhow::anyhow!("Cannot specify both --proof-url and --proof-file"));
        }
        (None, None) => {
            return Err(anyhow::anyhow!("Must specify either --proof-url or --proof-file"));
        }
    };
    
    tracing::info!("Proof data loaded, size: {} bytes", proof_data.len());
    
    // Parse vk_hash
    let vk_bytes = hex::decode(&vk_string)
        .map_err(|e| anyhow::anyhow!("Invalid vk_hash hex: {}", e))?;
    let vk: SP1VerifyingKey = bincode::deserialize(&vk_bytes)
        .map_err(|e| anyhow::anyhow!("Failed to parse verifying key: {}", e))?;
    let proof: SP1ProofWithPublicValues = bincode::deserialize(&proof_data)
        .map_err(|e| anyhow::anyhow!("Failed to parse proof data: {}", e))?;
    
    let client = ProverClient::from_env();
    match client.verify(&proof, &vk) {
        Ok(_) => tracing::info!("Proof verified successfully."),
        Err(e) => tracing::error!("Failed to verify proof: {}", e),
    }

    Ok(())
}

/// Load proof data from a URL
async fn load_proof_from_url(url: String) -> Result<Vec<u8>> {
    tracing::info!("Fetching proof from URL: {}", url);
    
    let response = reqwest::get(&url).await
        .map_err(|e| anyhow::anyhow!("Failed to fetch proof from URL: {}", e))?;
    
    if !response.status().is_success() {
        return Err(anyhow::anyhow!("Failed to fetch proof: HTTP {}", response.status()));
    }
    
    let proof_data = response.bytes().await
        .map_err(|e| anyhow::anyhow!("Failed to read proof data: {}", e))?
        .to_vec();
    
    tracing::info!("Successfully downloaded proof, size: {} bytes", proof_data.len());
    Ok(proof_data)
}

/// Load proof data from a binary file
async fn load_proof_from_file(file_path: String) -> Result<Vec<u8>> {
    tracing::info!("Reading proof from file: {}", file_path);
    
    let proof_data = tokio::fs::read(&file_path).await
        .map_err(|e| anyhow::anyhow!("Failed to read proof file '{}': {}", file_path, e))?;
    // let proof_data: SP1ProofWithPublicValues = SP1ProofWithPublicValues::load(file_path)?;

    tracing::info!("Successfully read proof file, size: {} bytes", proof_data.len());
    Ok(proof_data)
}
