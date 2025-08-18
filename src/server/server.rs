use anyhow::Result;
use rpc_types::*;
use tokio::sync::mpsc;
use tonic::transport::{Server, ServerTlsConfig, Identity};
use tonic_reflection::server::{Builder as ReflBuilder};

use crate::server::prover_network_service::ProverNetworkServiceImpl;
use crate::server::artifacts_service::ArtifactStoreServiceImpl;
use crate::server::http_server::HttpServer;

const PROTOS: &[u8] = include_bytes!("../../crates/types/rpc/src/generated/descriptor.bin");

/// Run both gRPC server and HTTP server concurrently
pub async fn run_server(mut shutdown_rx: mpsc::Receiver<()>) -> Result<()> {
    println!("=== Starting gRPC Server and HTTP Server ===");
    
    let grpc_addr = "0.0.0.0:50051".parse()?;
    let http_port = 8082;
    let tls_activated = false; // Set to true if TLS is enabled
    let prover_network_service = ProverNetworkServiceImpl::default();
    let artifacts_service = ArtifactStoreServiceImpl::default();
    
    // build a descriptor set at compile-time with prost-build / tonic-prost-build
    // then include it here (PROTOS is &[u8])
    let reflection = ReflBuilder::configure()
        .register_encoded_file_descriptor_set(PROTOS)
        .build_v1()?;

    println!("GRPC Server listening on {}", grpc_addr);
    println!("HTTP Server listening on port {}", http_port);
    println!("Servers will run until shutdown signal is received...");
    
    let cert = tokio::fs::read("testing-cert/server.pem").await?;
    let key  = tokio::fs::read("testing-cert/server.key").await?;
    let identity = Identity::from_pem(cert, key);

    // Create a real tonic gRPC server with both services
    let mut server = Server::builder();
    if tls_activated == true {
        server = server.tls_config(ServerTlsConfig::new().identity(identity))?;
    }
    
    // Start the gRPC server
    let grpc_server = server.add_service(prover_network_server::ProverNetworkServer::new(prover_network_service))
        .add_service(artifact_store_server::ArtifactStoreServer::new(artifacts_service))
        .add_service(reflection)
        .serve_with_shutdown(grpc_addr, async {
            let _ = shutdown_rx.recv().await;
            println!("Shutdown signal received, gracefully stopping gRPC server...");
        });

    // Start HTTP server in a separate task
    let http_server_handle = tokio::spawn(async move {
        let http_server = HttpServer::new(http_port);
        if let Err(e) = http_server.start().await {
            eprintln!("HTTP server error: {}", e);
        }
    });

    // Run gRPC server and wait for it to complete
    if let Err(e) = grpc_server.await {
        eprintln!("gRPC server error: {}", e);
    }

    // Abort the HTTP server task when gRPC server finishes
    http_server_handle.abort();
    
    println!("Servers shutdown complete");
    Ok(())
}