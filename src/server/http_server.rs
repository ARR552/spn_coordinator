use std::sync::Arc;
use bytes::Bytes;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, put},
    Router,
};
use rocksdb::{DB, Options, ColumnFamilyDescriptor};
use anyhow::Result;

/// HTTP server for handling artifact uploads via PUT requests
#[derive(Debug, Clone)]
pub struct HttpServer {
    /// RocksDB storage for uploaded artifacts
    pub storage: Arc<DB>,
    pub addr: String,
    pub port: u16,
}

impl HttpServer {
    pub fn new(addr: String, port: u16) -> Result<Self> {
        let db_path = "http_artifacts_db";
        
        // Define column families for each artifact type (same as in artifacts_service.rs)
        let cf_names = ["program", "stdin", "proof", "transaction", "unspecified"];
        let mut cf_descriptors = Vec::new();
        
        for cf_name in &cf_names {
            cf_descriptors.push(ColumnFamilyDescriptor::new(*cf_name, Options::default()));
        }
        
        let mut db_opts = Options::default();
        db_opts.create_missing_column_families(true);
        db_opts.create_if_missing(true);
        
        let db = DB::open_cf_descriptors(&db_opts, db_path, cf_descriptors)
            .map_err(|e| anyhow::anyhow!("Failed to open RocksDB: {}", e))?;
        
        Ok(Self {
            storage: Arc::new(db),
            addr,
            port,
        })
    }
    
    fn get_table_name_for_artifact_type(artifact_type: &str) -> Result<&'static str, StatusCode> {
        match artifact_type {
            "Program" => Ok("program"),
            "Stdin" => Ok("stdin"), 
            "Proof" => Ok("proof"),
            "Transaction" => Ok("transaction"),
            _ => {
                tracing::error!("HTTP: Invalid artifact type: {}", artifact_type);
                Err(StatusCode::BAD_REQUEST)
            }
        }
    }

    /// Start the HTTP server that handles PUT requests
    pub async fn start(self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let storage = self.storage.clone();
        
        // Build the application with routes
        let app = Router::new()
            .route("/artifacts/{artifact_type}/{artifact_id}", put(upload_artifact))
            .route("/artifacts/{artifact_type}/{artifact_id}", get(download_artifact))
            .route("/health", get(health_check))
            .with_state(storage);

        let url = format!("{}:{}", self.addr, self.port);
        tracing::info!("HTTP: Starting HTTP server on {}", url);

        let listener = tokio::net::TcpListener::bind(&url).await?;
        axum::serve(listener, app).await?;

        Ok(())
    }

    /// Get the storage reference for integration with other services
    pub fn get_storage(&self) -> Arc<DB> {
        self.storage.clone()
    }
}

/// Handler for PUT /artifacts/:artifact_id
async fn upload_artifact(
    Path((artifact_type, artifact_id)): Path<(String, String)>,
    State(storage): State<Arc<DB>>,
    body: Bytes,
) -> Result<&'static str, StatusCode> {
    tracing::info!("HTTP: Received PUT request for artifact: {}/{}", artifact_type, artifact_id);
    tracing::debug!("HTTP: Body size: {} bytes", body.len());
    // Get the appropriate column family for this artifact type
    let column_family_name = HttpServer::get_table_name_for_artifact_type(&artifact_type)?;
    let column_family_handle = storage.cf_handle(column_family_name)
        .ok_or_else(|| {
            tracing::error!("HTTP: Column family '{}' not found for artifact type '{}'", column_family_name, artifact_type);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    
    // Store the bytes using artifact_id as key in the appropriate column family
    storage.put_cf(&column_family_handle, artifact_id.as_bytes(), &body)
        .map_err(|e| {
            tracing::error!("HTTP: Failed to store artifact {}: {}", artifact_id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    
    tracing::debug!("HTTP: Successfully stored artifact: {} in column family: {}", artifact_id, column_family_name);
    
    Ok("Artifact uploaded successfully")
}

/// Handler for GET /artifacts/:artifact_id
async fn download_artifact(
    Path((artifact_type, artifact_id)): Path<(String, String)>,
    State(storage): State<Arc<DB>>,
) -> Result<(StatusCode, Vec<u8>), StatusCode> {
    tracing::info!("HTTP: Received GET request for artifact: {}/{}", artifact_type, artifact_id);
    // Get the appropriate column family for this artifact type
    let column_family_name = HttpServer::get_table_name_for_artifact_type(&artifact_type)?;
    let column_family_handle = storage.cf_handle(column_family_name)
        .ok_or_else(|| {
            tracing::error!("HTTP: Column family '{}' not found for artifact type '{}'", column_family_name, artifact_type);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    
    // Retrieve using artifact_id as key from the appropriate column family
    match storage.get_cf(&column_family_handle, artifact_id.as_bytes()) {
        Ok(Some(data)) => {
            tracing::debug!("HTTP: Found artifact: {} ({} bytes) in column family: {}", artifact_id, data.len(), column_family_name);
            Ok((StatusCode::OK, data.to_vec()))
        }
        Ok(None) => {
            tracing::error!("HTTP: Artifact not found: {} in column family: {}", artifact_id, column_family_name);
            Err(StatusCode::NOT_FOUND)
        }
        Err(e) => {
            tracing::error!("HTTP: Failed to retrieve artifact {}: {}", artifact_id, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Handler for GET /health
async fn health_check() -> &'static str {
    tracing::debug!("HTTP: Health check requested");
    "OK"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_storage_operations() {
        let server = HttpServer::new("0.0.0.0".to_string(), 0).expect("Failed to create server"); // Use port 0 for testing
        let storage = server.get_storage();
        
        let test_data = Bytes::from("test data");
        let artifact_id = "test_artifact_123".to_string();
        // Get column family handle
        let column_family_name = HttpServer::get_table_name_for_artifact_type("Program").expect("Invalid artifact type");
        let cf_handle = storage.cf_handle(column_family_name).expect("Column family not found");
        
        // Store data using column family
        storage.put_cf(&cf_handle, artifact_id.as_bytes(), &test_data).expect("Failed to store data");
        
        // Retrieve data using column family
        let retrieved = storage.get_cf(&cf_handle, artifact_id.as_bytes()).expect("Failed to get data");
        assert_eq!(retrieved, Some(test_data.to_vec()));
    }
}
