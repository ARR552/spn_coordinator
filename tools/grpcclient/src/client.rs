use anyhow::Result;
use rpc_types::*;
use std::time::Duration;
use tonic::{Request, Response, Status, transport::{Channel, Endpoint}};
use clap::Parser;

use crate::commands::{create_program::run_create_program, run_get_program, run_proof_request_details, run_proof_request_status, run_verify_proof};

/// Real gRPC client that makes actual gRPC calls
pub struct Client {
    prover_network_client: prover_network_client::ProverNetworkClient<Channel>,
    artifact_store_client: artifact_store_client::ArtifactStoreClient<Channel>
}

impl Client {
    pub async fn new(url: String) -> Result<Self, Box<dyn std::error::Error>> {
        let prover_network_channel: Channel = Endpoint::new(url.clone())?
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(15))
            .keep_alive_while_idle(true)
            .http2_keep_alive_interval(Duration::from_secs(15))
            .keep_alive_timeout(Duration::from_secs(15))
            .tcp_keepalive(Some(Duration::from_secs(30)))
            .connect()
            .await?;

        let artifact_store_channel: Channel = Endpoint::new(url.clone())?
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(15))
            .keep_alive_while_idle(true)
            .http2_keep_alive_interval(Duration::from_secs(15))
            .keep_alive_timeout(Duration::from_secs(15))
            .tcp_keepalive(Some(Duration::from_secs(30)))
            .connect()
            .await?;

        let prover_network_client = prover_network_client::ProverNetworkClient::new(prover_network_channel);
        let artifact_store_client = artifact_store_client::ArtifactStoreClient::new(artifact_store_channel);
        Ok(Self { prover_network_client: prover_network_client, artifact_store_client: artifact_store_client })
    }
    
    pub async fn request_proof(
        &mut self,
        request: RequestProofRequest,
    ) -> Result<Response<RequestProofResponse>, Status> {
        self.prover_network_client.request_proof(Request::new(request)).await
    }
    
    pub async fn get_proof_request_status(
        &mut self,
        request: GetProofRequestStatusRequest,
    ) -> Result<Response<GetProofRequestStatusResponse>, Status> {
        self.prover_network_client.get_proof_request_status(Request::new(request)).await
    }

    pub async fn get_proof_request_details(
        &mut self,
        request: GetProofRequestDetailsRequest,
    ) -> Result<Response<GetProofRequestDetailsResponse>, Status> {
        self.prover_network_client.get_proof_request_details(Request::new(request)).await
    }

    pub async fn get_program(
        &mut self,
        request: GetProgramRequest,
    ) -> Result<Response<GetProgramResponse>, Status> {
        self.prover_network_client.get_program(Request::new(request)).await
    }

    pub async fn create_program(
        &mut self,
        request: CreateProgramRequest,
    ) -> Result<Response<CreateProgramResponse>, Status> {
        self.prover_network_client.create_program(Request::new(request)).await
    }

    pub async fn create_artifact(
        &mut self,
        request: CreateArtifactRequest,
    ) -> Result<Response<CreateArtifactResponse>, Status> {
        self.artifact_store_client.create_artifact(Request::new(request)).await
    }
}

/// Client function that connects to the server
pub async fn run_client() -> Result<()> {
    // Wait a bit for server to start
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    #[derive(Parser)]
    #[command(name = "grpc-client")]
    #[command(about = "A gRPC client for ProverNetwork")]
    struct Cli {
        #[command(subcommand)]
        command: Commands,
    }

    #[derive(Parser)]
    enum Commands {
        /// Get proof request details
        ProofRequestDetails {
            #[arg(long, default_value = "https://rpc-production.succinct.xyz")]
            url: String,
            #[arg(long, required = true)]
            request_id: String,
        },
        /// Get proof request status
        ProofRequestStatus {
            #[arg(long, default_value = "https://rpc-production.succinct.xyz")]
            url: String,
            #[arg(long, required = true)]
            request_id: String,
        },
        /// Get program information
        GetProgram {
            #[arg(long, default_value = "https://rpc-production.succinct.xyz")]
            url: String,
            #[arg(long, required = true)]
            vk_hash: String,
        },
        /// Verify a proof
        VerifyProof {
            #[arg(long, group = "proof_source")]
            proof_url: Option<String>,
            #[arg(long, group = "proof_source")]
            proof_file: Option<String>,
            #[arg(long, required = true)]
            vk: String,
        },
        /// Create a program
        CreateProgram {
            #[arg(long, required = true)]
            url: String,
            #[arg(long, required = true)]
            private_key: String,
            #[arg(long, required = true)]
            elf_path: String,
        },
    }

    let cli = Cli::parse();
    
    match cli.command {
        Commands::ProofRequestDetails { url, request_id } => {
            run_proof_request_details(url, request_id).await?;
        }
        Commands::ProofRequestStatus { url, request_id } => {
            run_proof_request_status(url, request_id).await?;
        }
        Commands::GetProgram { url, vk_hash } => {
            run_get_program(url, vk_hash).await?;
        }
        Commands::VerifyProof { proof_url, proof_file, vk } => {
            run_verify_proof(proof_url, proof_file, vk).await?;
        }
        Commands::CreateProgram { url, private_key, elf_path } => {
            run_create_program(url, private_key, elf_path).await?;
        }
    }
    
    Ok(())
}
