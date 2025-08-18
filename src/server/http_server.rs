use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use bytes::Bytes;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, put},
    Router,
};

/// HTTP server for handling artifact uploads via PUT requests
#[derive(Debug, Clone)]
pub struct HttpServer {
    /// In-memory storage for uploaded artifacts
    pub storage: Arc<Mutex<HashMap<String, Bytes>>>,
    pub port: u16,
}

impl HttpServer {
    pub fn new(port: u16) -> Self {
        Self {
            storage: Arc::new(Mutex::new(HashMap::new())),
            port,
        }
    }

    /// Start the HTTP server that handles PUT requests
    pub async fn start(self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let storage = self.storage.clone();
        
        // Build the application with routes
        let app = Router::new()
            .route("/artifacts/{artifact_id}", put(upload_artifact))
            .route("/artifacts/{artifact_id}", get(download_artifact))
            .route("/health", get(health_check))
            .with_state(storage);

        let addr = format!("0.0.0.0:{}", self.port);
        println!("HTTP: Starting HTTP server on {}", addr);

        let listener = tokio::net::TcpListener::bind(&addr).await?;
        axum::serve(listener, app).await?;

        Ok(())
    }

    /// Get the storage reference for integration with other services
    pub fn get_storage(&self) -> Arc<Mutex<HashMap<String, Bytes>>> {
        self.storage.clone()
    }
}

/// Handler for PUT /artifacts/:artifact_id
async fn upload_artifact(
    Path(artifact_id): Path<String>,
    State(storage): State<Arc<Mutex<HashMap<String, Bytes>>>>,
    body: Bytes,
) -> Result<&'static str, StatusCode> {
    println!("HTTP: Received PUT request for artifact: {}", artifact_id);
    println!("HTTP: Body size: {} bytes", body.len());
    
    // Store the bytes in memory
    storage.lock().await.insert(artifact_id.clone(), body);
    
    println!("HTTP: Successfully stored artifact: {}", artifact_id);
    
    Ok("Artifact uploaded successfully")
}

/// Handler for GET /artifacts/:artifact_id
async fn download_artifact(
    Path(artifact_id): Path<String>,
    State(storage): State<Arc<Mutex<HashMap<String, Bytes>>>>,
) -> Result<(StatusCode, Vec<u8>), StatusCode> {
    println!("HTTP: Received GET request for artifact: {}", artifact_id);
    
    let storage_guard = storage.lock().await;
    if let Some(data) = storage_guard.get(&artifact_id) {
        println!("HTTP: Found artifact: {} ({} bytes)", artifact_id, data.len());
        Ok((StatusCode::OK, data.to_vec()))
    } else {
        println!("HTTP: Artifact not found: {}", artifact_id);
        Err(StatusCode::NOT_FOUND)
    }
}

/// Handler for GET /health
async fn health_check() -> &'static str {
    println!("HTTP: Health check requested");
    "OK"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_storage_operations() {
        let server = HttpServer::new(0); // Use port 0 for testing
        let storage = server.get_storage();
        
        let test_data = Bytes::from("test data");
        let artifact_id = "test_artifact_123".to_string();
        
        // Store data
        storage.lock().await.insert(artifact_id.clone(), test_data.clone());
        
        // Retrieve data
        let retrieved = storage.lock().await.get(&artifact_id).cloned();
        assert_eq!(retrieved, Some(test_data));
    }
}
