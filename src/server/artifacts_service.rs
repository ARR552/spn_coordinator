use anyhow::Result;
use rpc_types::*;
use std::sync::Arc;
use tonic::{Request, Response, Status};
use rand::random;
use rocksdb::{DB, Options, ColumnFamilyDescriptor};

/// Real gRPC service implementation for ArtifactStore
pub struct ArtifactStoreServiceImpl {
    // Base URL for artifact uploads (e.g., "http://localhost:8082")
    pub artifact_base_url: String,
    /// RocksDB database with separate column families for each artifact type
    db: Arc<DB>,
}

impl ArtifactStoreServiceImpl {
    pub fn new(artifact_base_url: String) -> Result<Self> {
        let db_path = "artifacts_db";
        
        // Define column families for each artifact type
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
            artifact_base_url,
            db: Arc::new(db),
        })
    }
    
    fn get_table_name_for_artifact_type(artifact_type: &ArtifactType) -> &'static str {
        match artifact_type {
            ArtifactType::Program => "program",
            ArtifactType::Stdin => "stdin", 
            ArtifactType::Proof => "proof",
            ArtifactType::Transaction => "transaction",
            ArtifactType::UnspecifiedArtifactType => "unspecified",
        }
    }
}

#[tonic::async_trait]
impl artifact_store_server::ArtifactStore for ArtifactStoreServiceImpl {
    /// Creates an artifact that can be used for proof requests.
    async fn create_artifact(
        &self,
        request: Request<CreateArtifactRequest>,
    ) -> Result<Response<CreateArtifactResponse>, Status> {
        let req = request.into_inner();
        tracing::debug!("ARTIFACT: Server received create_artifact request with signature: {:?}", hex::encode(&req.signature));
        
        // Validate the artifact type
        let artifact_type = ArtifactType::try_from(req.artifact_type)
            .map_err(|_| Status::invalid_argument("Invalid artifact type"))?;
        
        // TODO: Verify signature for authentication
        // For now, we'll skip signature verification as it would require the signed message format
        
        // Generate unique artifact URI and presigned URL
        let artifact_id = generate_artifact_id();
        // let artifact_uri = generate_artifact_uri(&artifact_type, &artifact_id);
        let presigned_url = generate_presigned_url(&self.artifact_base_url, &artifact_type, &artifact_id);
        let artifact_uri = presigned_url.clone();

        tracing::info!("ARTIFACT: Generated presigned URL: {}", presigned_url);
        
        // Store the artifact metadata in RocksDB
        let column_family_name = Self::get_table_name_for_artifact_type(&artifact_type);
        let artifact_column_family_handle = self.db.cf_handle(column_family_name)
            .ok_or_else(|| Status::internal(format!("Column family '{}' not found", column_family_name)))?;
        
        // Serialize the presigned URL as the value
        let value = presigned_url.as_bytes();
        self.db.put_cf(&artifact_column_family_handle, artifact_id.as_bytes(), value)
            .map_err(|e| Status::internal(format!("Failed to store artifact: {}", e)))?;
        
        let response = CreateArtifactResponse {
            artifact_uri: artifact_uri.clone(),
            artifact_presigned_url: presigned_url,
        };
        
        tracing::info!("ARTIFACT: Successfully created artifact: {}", artifact_uri);
        Ok(Response::new(response))
    }
}

impl ArtifactStoreServiceImpl {
    /// Get artifact metadata by artifact_id and type
    pub fn get_artifact(&self, artifact_type: &ArtifactType, artifact_id: &str) -> Result<Option<String>, Status> {
        // column family in rocksDB == table in sql
        let column_family_name = Self::get_table_name_for_artifact_type(artifact_type);
        let artifact_column_family_handle = self.db.cf_handle(column_family_name)
            .ok_or_else(|| Status::internal(format!("Column family '{}' not found", column_family_name)))?;
        
        match self.db.get_cf(&artifact_column_family_handle, artifact_id.as_bytes()) {
            Ok(Some(value)) => {
                let presigned_url = String::from_utf8(value)
                    .map_err(|e| Status::internal(format!("Failed to decode presigned URL: {}", e)))?;
                Ok(Some(presigned_url))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(Status::internal(format!("Failed to retrieve artifact: {}", e))),
        }
    }
}

/// Generate a unique artifact identifier
fn generate_artifact_id() -> String {
    let id_bytes = random::<[u8; 16]>();
    hex::encode(id_bytes)
}

/// Generate an artifact URI based on the ID
// fn generate_artifact_uri(artifact_type: &ArtifactType, artifact_id: &str) -> String {    
//     format!("s3://spn-artifacts/{:?}/{}", artifact_type, artifact_id)
// }

/// Generate a presigned URL for artifact upload
fn generate_presigned_url(artifact_base_url: &str, artifact_type: &ArtifactType, artifact_id: &str) -> String {
    // Generate a URL pointing to our HTTP server
    // The client will use this URL to PUT the artifact data
    format!("{}/artifacts/{:?}/{}", artifact_base_url, artifact_type, artifact_id)
}
