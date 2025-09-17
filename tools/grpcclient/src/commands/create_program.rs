use anyhow::Result;
use bincode;
use rpc_types::*;
use prost::Message;
use ethers::{utils::keccak256};
use ethers::signers::{LocalWallet, Signer};
use std::str::FromStr;
use std::{sync::Arc};
use std::fs;
use sp1_sdk::{
    HashableKey, Prover, ProverClient, SP1VerifyingKey,
};
use crate::client::Client;

pub async fn run_create_program(
    grpc_url: String, 
    private_key: String, 
    elf_path: String, 
) -> Result<()> {
    tracing::info!("=== Run create_program ===");
    let artifact = fs::read(&elf_path)?;

    let mut grpc_client = Client::new(grpc_url.to_string()).await
        .map_err(|e| {
            tracing::error!("Detailed prover_network_client creation error: {:?}", e);
            anyhow::anyhow!("Failed to create prover_network_client: {}", e)
        })?;

    let artifact_type = ArtifactType::Program;
    let artifact_request = create_artifact_request(artifact_type).await?;
    
    let response_inner = match grpc_client.create_artifact(artifact_request).await {
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
    tracing::info!("Uploading artifact ({} bytes) to presigned URL...", artifact.len());

    let put_url = response_inner.artifact_presigned_url.clone();
    let http_client = reqwest::Client::new();
    let upload_response = http_client
        .put(put_url.clone())
        .header("Content-Type", "application/binary")
        .body(artifact.clone())
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
    let request = create_program_request(response_inner.artifact_presigned_url.clone(), artifact.clone(), &private_key).await?;
    
    tracing::info!("Client sending proof request ");
    // let response = client.request_proof(request).await?;
    let response = grpc_client.create_program(request).await?;
    let response_inner = response.into_inner();
    
    tracing::info!("Client create program response: TX Hash = {}", hex::encode(&response_inner.tx_hash));

    Ok(())
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

pub async fn create_program_request(program_uri: String, elf: Vec<u8>, private_key: &str) -> anyhow::Result<CreateProgramRequest> {
    let network_prover =
            Arc::new(ProverClient::builder().network().private_key(&private_key).build());
    let elf_bytes: &[u8] = &elf;
    let (_proving_key, verification_key) = network_prover.setup(elf_bytes);
    let program = rpc_types::CreateProgramRequestBody {
        vk_hash: verification_key.vk.bytes32_raw().to_vec(), //hex::decode("005d763c1b4e00563d156f9ba8cc60561014267a5d3f5f16e2b8a47fa9dfe173").unwrap_or_default(),
        vk: bincode::serialize(&verification_key)?, //hex::decode("18c19a61c29c213edfea9e0e5f7b35610f968f43282c5002be4fd123980b3a4644a92d00fecded6ac7efd272fca32d3f487d864ef12bf638be069326153b79650edd32370c739032ac70962f7b08ef1376627c701343d63742584c2c0200000000000000070000000000000050726f6772616d1400000000000000010000000e0000000000000000001000000000000400000000000000427974651000000000000000010000000b0000000000000000000100000000000200000000000000070000000000000050726f6772616d00000000000000000400000000000000427974650100000000000000").unwrap_or_default(),
        program_uri: program_uri,
        nonce: 0,
    };
    let vk1: SP1VerifyingKey = bincode::deserialize(&program.vk)?;
    let computed_vk_hash = vk1.bytes32_raw();
    tracing::info!("program.vk_hash: {:?}", hex::encode(program.vk_hash.clone()));
    tracing::info!("program.vk: {:?}", hex::encode(program.vk.clone()));
    if hex::encode(computed_vk_hash) != hex::encode(program.vk_hash.clone()) {
        tracing::error!("computed_vk_hash: {}, vk_hash: {}", hex::encode(computed_vk_hash), hex::encode(program.vk_hash.clone()));
        return Err(anyhow::anyhow!("VK hash mismatch!"));
    }
    let mut buf = Vec::new();
    let wallet = LocalWallet::from_str(private_key)?;
    tracing::info!("Client Wallet address: {}", wallet.address());
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