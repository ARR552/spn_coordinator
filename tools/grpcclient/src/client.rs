use anyhow::Result;
use rpc_types::*;
use std::time::Duration;
use tonic::{Request, Response, Status, transport::{Channel, Endpoint}};
use clap::Parser;

use crate::commands::{run_proof_request_details, run_proof_request_status};

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
            #[arg(long, default_value = "4e94a6a152d166b9c26faf27e406ead95b60aee02da50294e10a46131fbb9f5f")]
            request_id: String,
        },
        /// Get proof request status
        ProofRequestStatus {
            #[arg(long, default_value = "https://rpc-production.succinct.xyz")]
            url: String,
            #[arg(long, default_value = "4e94a6a152d166b9c26faf27e406ead95b60aee02da50294e10a46131fbb9f5f")]
            request_id: String,
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
    }
    
    Ok(())
}
