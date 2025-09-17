use anyhow::Result;
use rpc_types::*;
use tonic::{Request, Response, Status};
use ethers_core::types::{Signature};
use ethers_core::utils::hash_message; // adds the EIP-191 prefix
use eyre;
use rand::random;
use prost::Message;
use std::sync::Arc;
use rocksdb::{DB, Options, ColumnFamilyDescriptor};
use tokio::sync::{mpsc, Mutex};
use tokio::task;
use crate::config::ServerConfig;

/// Real gRPC service implementation for ProverNetwork
#[derive(Debug)]
pub struct ProverNetworkServiceImpl {
    // Base URL for artifact uploads (e.g., "http://localhost:8082")
    pub artifact_base_url: String,
    /// RocksDB database with separate column families for proof requests and programs
    db: Arc<DB>,
    /// Channel to notify background assignment process of available provers
    free_prover_tx: mpsc::UnboundedSender<Vec<u8>>,
}

impl ProverNetworkServiceImpl {
    pub fn new(cfg: ServerConfig, artifact_base_url: String) -> Result<Self> {
        let db_path = cfg.db_path + "prover_network_db";
        
        // Define column families for proof requests and programs
        let cf_names = ["proof_requests", "programs"];
        let mut cf_descriptors = Vec::new();
        
        for cf_name in &cf_names {
            cf_descriptors.push(ColumnFamilyDescriptor::new(*cf_name, Options::default()));
        }
        
        let mut db_opts = Options::default();
        db_opts.create_missing_column_families(true);
        db_opts.create_if_missing(true);
        
        let db = DB::open_cf_descriptors(&db_opts, db_path, cf_descriptors)
            .map_err(|e| anyhow::anyhow!("Failed to open RocksDB: {}", e))?;
        
        let db = Arc::new(db);
        
        // Create channel for notifying about free provers
        let (free_prover_tx, mut free_prover_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        
        // Create mutex for assignment process
        let assignment_mutex = Arc::new(Mutex::new(()));
        
        // Clone everything needed for background tasks
        let db_clone = Arc::clone(&db);
        let assignment_mutex_clone = Arc::clone(&assignment_mutex);
        let db_timeout_clone = Arc::clone(&db);
        let assignment_mutex_timeout_clone = Arc::clone(&assignment_mutex);
        
        // Spawn background task for automatic proof assignment
        task::spawn(async move {
            while let Some(prover_address) = free_prover_rx.recv().await {
                if let Err(e) = assign_proof_to_prover_static(&db_clone, &prover_address, &assignment_mutex_clone).await {
                    tracing::error!("Failed to assign proof to prover {:?}: {}", hex::encode(&prover_address), e);
                }
            }
        });
        
        let proof_reassign_timeout: u64 = cfg.checker.proof_reassign_timeout;
        let checker_interval:u64 = cfg.checker.checker_interval;
        // Spawn background task for timeout checking (every 30 seconds)
        task::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(checker_interval));
            loop {
                interval.tick().await;
                if let Err(e) = check_and_reassign_expired_proofs(&db_timeout_clone, &assignment_mutex_timeout_clone, proof_reassign_timeout).await {
                    tracing::error!("Failed to check for expired proof assignments: {}", e);
                }
            }
        });
        
        Ok(Self {
            artifact_base_url,
            db,
            free_prover_tx,
        })
    }

    // Helper method to store proof request data
    fn add_proof_request(&self, request_id: &[u8], proof_request: &ProofRequest, status_response: &GetProofRequestStatusResponse) -> Result<(), Status> {
        let table_name = "proof_requests";
        let column_family_handle = self.db.cf_handle(table_name)
            .ok_or_else(|| Status::internal("Column family 'proof_requests' not found"))?;
        // Serialize the data as a tuple (ProofRequest, GetProofRequestStatusResponse)
        let data = (proof_request, status_response);
        let serialized_data = bincode::serialize(&data)
            .map_err(|e| Status::internal(format!("Failed to serialize proof request data: {}", e)))?;
        
        self.db.put_cf(&column_family_handle, request_id, serialized_data)
            .map_err(|e| Status::internal(format!("Failed to store proof request: {}", e)))?;
        
        Ok(())
    }

    fn store_proof_request(&self, request_id: &[u8], proof_request: &ProofRequest, status_response: &GetProofRequestStatusResponse) -> Result<(), Status> {
        let table_name = "proof_requests";
        let column_family_handle = self.db.cf_handle(table_name)
            .ok_or_else(|| Status::internal("Column family 'proof_requests' not found"))?;
        if self.db.key_may_exist_cf(column_family_handle, request_id) {
            tracing::warn!("Proofrequest {:?} already exists", request_id);
            return Err(Status::already_exists(format!("Proofrequest with request_id {:?} already exists", request_id)));
        }
        self.add_proof_request(&request_id, &proof_request, &status_response)
    }

    // Helper method to retrieve proof request data
    fn get_proof_request(&self, request_id: &[u8]) -> Result<Option<(ProofRequest, GetProofRequestStatusResponse)>, Status> {
        let table_name = "proof_requests";
        let column_family_handle = self.db.cf_handle(table_name)
            .ok_or_else(|| Status::internal("Column family 'proof_requests' not found"))?;
        
        match self.db.get_cf(&column_family_handle, request_id) {
            Ok(Some(value)) => {
                let data: (ProofRequest, GetProofRequestStatusResponse) = bincode::deserialize(&value)
                    .map_err(|e| Status::internal(format!("Failed to deserialize proof request data: {}", e)))?;
                Ok(Some(data))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(Status::internal(format!("Failed to retrieve proof request: {}", e))),
        }
    }

    // Helper method to update proof request data
    fn update_proof_request(&self, request_id: &[u8], proof_request: &ProofRequest, status_response: &GetProofRequestStatusResponse) -> Result<(), Status> {
        self.add_proof_request(request_id, proof_request, status_response)
    }

    // Helper method to store program data
    fn store_program(&self, vk_hash: &[u8], program: &Program) -> Result<(), Status> {
        let table_name = "programs";
        let column_family_handle = self.db.cf_handle(table_name)
            .ok_or_else(|| Status::internal("Column family 'programs' not found"))?;
        if self.db.key_may_exist_cf(column_family_handle, vk_hash) {
            tracing::warn!("Program {:?} already exists", hex::encode(vk_hash));
            return Err(Status::already_exists(format!("Program with vk_hash {:?} already exists", hex::encode(vk_hash))));
        }
        let serialized_program = bincode::serialize(program)
            .map_err(|e| Status::internal(format!("Failed to serialize program: {}", e)))?;
        
        self.db.put_cf(&column_family_handle, vk_hash, serialized_program)
            .map_err(|e| Status::internal(format!("Failed to store program: {}", e)))?;
        
        Ok(())
    }

    // Helper method to retrieve program data
    fn get_program_by_vk_hash(&self, vk_hash: &[u8]) -> Result<Option<Program>, Status> {
        let table_name = "programs";
        let column_family_handle = self.db.cf_handle(table_name)
            .ok_or_else(|| Status::internal("Column family 'programs' not found"))?;
        
        match self.db.get_cf(&column_family_handle, vk_hash) {
            Ok(Some(value)) => {
                let program: Program = bincode::deserialize(&value)
                    .map_err(|e| Status::internal(format!("Failed to deserialize program: {}", e)))?;
                Ok(Some(program))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(Status::internal(format!("Failed to retrieve program: {}", e))),
        }
    }

    // Helper method to get all proof requests (for filtering operations)
    fn get_all_proof_requests(&self) -> Result<Vec<(Vec<u8>, ProofRequest, GetProofRequestStatusResponse)>, Status> {
        let table_name = "proof_requests";
        let column_family_handle = self.db.cf_handle(table_name)
            .ok_or_else(|| Status::internal("Column family 'proof_requests' not found"))?;
        
        let mut requests = Vec::new();
        let iter = self.db.iterator_cf(&column_family_handle, rocksdb::IteratorMode::Start);
        
        for item in iter {
            let (key, value) = item.map_err(|e| Status::internal(format!("Failed to iterate proof requests: {}", e)))?;
            let (proof_request, status_response): (ProofRequest, GetProofRequestStatusResponse) = bincode::deserialize(&value)
                .map_err(|e| Status::internal(format!("Failed to deserialize proof request data: {}", e)))?;
            requests.push((key.to_vec(), proof_request, status_response));
        }
        
        Ok(requests)
    }
}

#[tonic::async_trait]
impl prover_network_server::ProverNetwork for ProverNetworkServiceImpl {
    async fn request_proof(
        &self,
        request: Request<RequestProofRequest>,
    ) -> Result<Response<RequestProofResponse>, Status> {
        let req = request.into_inner();
        tracing::debug!("PROVER_NETWORK: Server Request params: {:?}", req);
        tracing::debug!("PROVER_NETWORK: Server Signature received: {:?}", hex::encode(&req.signature));
        
        // Generate a unique request ID
        let request_id = random::<[u8; 32]>().to_vec();
        tracing::info!("PROVER_NETWORK: Server Request_id: {:?}", hex::encode(&request_id));
        // Create a response
        let tx_hash_bytes = random::<[u8; 32]>().to_vec();
        let response = RequestProofResponse {
            tx_hash: tx_hash_bytes.clone(),
            body: Some(RequestProofResponseBody {
                request_id: request_id.clone(),
            }),
        };
        
        // Store the request for status tracking
        let status_response = GetProofRequestStatusResponse {
            fulfillment_status: FulfillmentStatus::Requested as i32,
            execution_status: ExecutionStatus::Unexecuted as i32,
            request_tx_hash: response.tx_hash.clone(),
            deadline: req.body.as_ref().map(|b| b.deadline).unwrap_or_default(),
            fulfill_tx_hash: None,
            proof_uri: None,
            public_values_hash: None,
            proof_public_uri: None,
        };
        let msg_bytes: Vec<u8> = encode_body_for_signing(req.format, req.body.as_ref().ok_or_else(|| Status::invalid_argument("Request body is required"))?)
            .map_err(|e| Status::internal(format!("Failed to encode body for signing: {}", e)))?;
        let requester = match req.body.as_ref() {
            Some(_body) => recover_signer_addr(msg_bytes, &req.signature)
                .map_err(|e| Status::invalid_argument(format!("Failed to recover signer address: {}", e)))?,
            None => return Err(Status::invalid_argument("Request body is required")),
        };
        tracing::info!("PROVER_NETWORK: Server Recovered requester address: {:?}", hex::encode(&requester));
        let now = chrono::Utc::now().timestamp() as u64;
        let vk_hash = req.body.as_ref().map(|b| b.vk_hash.clone()).unwrap_or_default();
        let program = self.get_program_by_vk_hash(&vk_hash).ok().flatten();
        let proof_request = ProofRequest {
                request_id: request_id.clone(),
                vk_hash: vk_hash,
                version: req.body.as_ref().map(|b| b.version.clone()).unwrap_or_default(),
                mode:    req.body.as_ref().map(|b| b.mode.clone()).unwrap_or_default(),
                strategy: req.body.as_ref().map(|b| b.strategy.clone()).unwrap_or_default(),
                deadline: req.body.as_ref().map(|b| b.deadline.clone()).unwrap_or_default(),
                cycle_limit: req.body.as_ref().map(|b| b.cycle_limit.clone()).unwrap_or_default(),
                fulfillment_status: status_response.fulfillment_status.clone(),
                execution_status: status_response.execution_status.clone(),
                created_at: now,
                updated_at: now,
                tx_hash: response.tx_hash.clone(),
                public_values_hash: req.body.as_ref().map(|b| b.public_values_hash.clone()).unwrap_or_default(),
                gas_limit: req.body.as_ref().map(|b| b.gas_limit.clone()).unwrap_or_default(),
                min_auction_period: req.body.as_ref().map(|b| b.min_auction_period.clone()).unwrap_or_default(),
                whitelist: req.body.as_ref().map(|b| b.whitelist.clone()).unwrap_or_default(),
                requester: requester.clone(),
                program_uri: program.as_ref().map(|p| p.program_uri.clone()).unwrap_or_default(),
                program_public_uri: program.as_ref().map(|p| p.program_uri.clone()).unwrap_or_default(),
                stdin_uri: req.body.as_ref().map(|b| b.stdin_uri.clone()).unwrap_or_default(),
                stdin_public_uri: req.body.as_ref().map(|b| b.stdin_uri.clone()).unwrap_or_default(),
                ..Default::default()
            };
        
        // Store the proof request in RocksDB
        self.store_proof_request(&request_id, &proof_request, &status_response)?;
        
        Ok(Response::new(response))
    }
    
    async fn get_proof_request_status(
        &self,
        request: Request<GetProofRequestStatusRequest>,
    ) -> Result<Response<GetProofRequestStatusResponse>, Status> {
        let req = request.into_inner();
        tracing::info!("PROVER_NETWORK: Server Received status request for ID: {:?}", hex::encode(&req.request_id));
        
        if let Some((_, status)) = self.get_proof_request(&req.request_id)? {
            Ok(Response::new(status))
        } else {
            Err(Status::not_found("Proof request not found"))
        }
    }

    // Implement all other required methods with unimplemented status for now
    async fn fulfill_proof(&self, request: Request<FulfillProofRequest>) -> Result<Response<FulfillProofResponse>, Status> {
        tracing::info!("PROVER_NETWORK: fulfill_proof method called");
        let req = request.into_inner();
        let msg_bytes: Vec<u8> = encode_body_for_signing(req.format, req.body.as_ref().ok_or_else(|| Status::invalid_argument("Request body is required"))?)
            .map_err(|e| Status::internal(format!("Failed to encode body for signing: {}", e)))?;
        let requester = match req.body.as_ref() {
            Some(_body) => recover_signer_addr(msg_bytes, &req.signature)
                .map_err(|e| Status::invalid_argument(format!("Failed to recover signer address: {}", e)))?,
            None => return Err(Status::invalid_argument("Request body is required")),
        };
        tracing::info!("PROVER_NETWORK: Server fulfill_proof method Recovered requester address: {:?}", hex::encode(&requester));

        let body = req.body.ok_or_else(|| Status::invalid_argument("Request body is required"))?;
        tracing::debug!("PROVER_NETWORK: domain: {}, request_id: {}, variant: {}, nonce: {}, reserved_metadata: {:?}", hex::encode(&body.domain), hex::encode(&body.request_id), body.variant, body.nonce, body.reserved_metadata);
        let tx_hash_bytes = random::<[u8; 32]>().to_vec();
        
        if let Some((mut proof_request, mut status)) = self.get_proof_request(&body.request_id)? {
            // Check signer
            if proof_request.fulfiller.as_ref() != Some(&requester) {
                tracing::error!("✗ Fulfiller address {:?} does not match assigned fulfiller address {:?}", hex::encode(&requester), proof_request.fulfiller.as_ref().map(|f| hex::encode(f)).unwrap_or_default());
                return Err(Status::permission_denied("Fulfiller address does not match assigned fulfiller address"));
            }
            // Upload proof
            let url = generate_proof_url(self.artifact_base_url.as_str());
            let client = reqwest::Client::new();
            let upload_response = client
                .put(url.clone())
                .header("Content-Type", "application/binary")
                .body(body.proof.clone())
                .send()
                .await
                .map_err(|e| Status::internal(format!("Failed to upload proof: {}", e)))?;

            if upload_response.status().is_success() {
                tracing::debug!("✓ Proof uploaded successfully!");
            } else {
                tracing::error!("✗ Failed to upload proof. Status: {}", upload_response.status());
                tracing::error!("Response: {:?}", upload_response.text().await);
                Err(Status::internal("Failed to upload proof"))?;
            }
            // Update fulfillment status to Fulfilled
            status.fulfillment_status = FulfillmentStatus::Fulfilled as i32;
            status.fulfill_tx_hash = Some(tx_hash_bytes.clone());
            status.proof_uri = Some(url.clone());
            status.proof_public_uri = Some(url.clone());
            status.execution_status = ExecutionStatus::Executed as i32;
            // status.public_values_hash = 

            let now = chrono::Utc::now().timestamp() as u64;
            proof_request.fulfillment_status = status.fulfillment_status;
            proof_request.updated_at = now;
            // proof_request.fulfiller = Some(requester);
            proof_request.fulfilled_at = Some(now);
            proof_request.execution_status = ExecutionStatus::Executed as i32;
            
            // Update the proof request in RocksDB
            self.update_proof_request(&body.request_id, &proof_request, &status)?;
            
            let response = FulfillProofResponse {
                tx_hash: tx_hash_bytes.clone(),
                body: Some(FulfillProofResponseBody {}),
            };
            return Ok(Response::new(response));
        }
        Err(Status::not_found("Proof request not found"))
    }

    async fn execute_proof(&self, _request: Request<ExecuteProofRequest>) -> Result<Response<ExecuteProofResponse>, Status> {
        tracing::info!("PROVER_NETWORK: execute_proof method called but not implemented");
        Err(Status::unimplemented("execute_proof not implemented"))
    }

    async fn fail_fulfillment(&self, request: Request<FailFulfillmentRequest>) -> Result<Response<FailFulfillmentResponse>, Status> {
        // Extract signature and recover requester address
        let req = request.into_inner();
        let msg_bytes: Vec<u8> = encode_body_for_signing(req.format, req.body.as_ref().ok_or_else(|| Status::invalid_argument("Request body is required"))?)
            .map_err(|e| Status::internal(format!("Failed to encode body for signing: {}", e)))?;
        let requester = match req.body.as_ref() {
            Some(_body) => recover_signer_addr(msg_bytes, &req.signature)
                .map_err(|e| Status::invalid_argument(format!("Failed to recover signer address: {}", e)))?,
            None => return Err(Status::invalid_argument("Request body is required")),
        };
        tracing::info!("PROVER_NETWORK: Server fulfill_proof method Recovered requester address: {:?}", hex::encode(&requester));

        // Extract body safely from Option
        let body = req.body.ok_or_else(|| Status::invalid_argument("Request body is required"))?;
        
        if let Some((mut proof_request, mut status)) = self.get_proof_request(&body.request_id)? {
            // Check signer
            if proof_request.fulfiller.as_ref() != Some(&requester) {
                tracing::error!("✗ Fulfiller address {:?} does not match assigned fulfiller address {:?}", hex::encode(&requester), proof_request.fulfiller.as_ref().map(|f| hex::encode(f)).unwrap_or_default());
                return Err(Status::permission_denied("Fulfiller address does not match assigned fulfiller address"));
            }
            // Update fulfillment status to Unfulfillable
            status.fulfillment_status = FulfillmentStatus::Unfulfillable as i32;
            let now = chrono::Utc::now().timestamp() as u64;
            proof_request.fulfillment_status = status.fulfillment_status;
            proof_request.updated_at = now;
            proof_request.error = body.error.unwrap_or(0); // Unwrap Option<i32> to i32, default to 0
            
            // Update the proof request in RocksDB
            self.update_proof_request(&body.request_id, &proof_request, &status)?;
            
            let response = FailFulfillmentResponse {
                tx_hash: proof_request.tx_hash.clone(),
                body: Some(FailFulfillmentResponseBody {}),
            };
            return Ok(Response::new(response));
        }
        Err(Status::not_found("Proof request not found"))
    }

    async fn get_proof_request_details(&self, _request: Request<GetProofRequestDetailsRequest>) -> Result<Response<GetProofRequestDetailsResponse>, Status> {
        tracing::info!("PROVER_NETWORK: Server received get_proof_request_details request");
        let req_inner = _request.into_inner();
        tracing::info!("PROVER_NETWORK: Request ID received: {:?}", hex::encode(&req_inner.request_id));
        
        if let Some((request, _)) = self.get_proof_request(&req_inner.request_id)? {            
            let response = GetProofRequestDetailsResponse {
                request: Some(request),
            };
            tracing::debug!("PROVER_NETWORK: Found request, returning details");
            Ok(Response::new(response))
        } else {
            tracing::debug!("PROVER_NETWORK: Request not found in storage");
            Err(Status::not_found("Proof request not found"))
        }
    }

    async fn get_filtered_proof_requests(&self, _request: Request<GetFilteredProofRequestsRequest>) -> Result<Response<GetFilteredProofRequestsResponse>, Status> {
        // Clone the request data for logging before consuming the request
        let request_data = _request.get_ref().clone();
        let req_inner = _request.into_inner();
        
        // Get all proof requests from RocksDB
        let all_requests = self.get_all_proof_requests()?;
        let mut filtered_requests: Vec<ProofRequest> = all_requests
            .into_iter()
            .map(|(_, req, _)| req)
            .filter(|req| {
            // Filter by requester if provided
            if let Some(ref filter_requester) = req_inner.requester {
            if !filter_requester.is_empty() && req.requester != *filter_requester {
                tracing::debug!("PROVER_NETWORK: Received get_filtered_proof_requests request: {:?}. Filtering by requester. Not matching: {:?}", request_data, req.requester);
                return false;
            }
            }
            
            // Filter by fulfillment status if provided
            if req_inner.fulfillment_status.is_some() && req.fulfillment_status != req_inner.fulfillment_status.unwrap() {
                tracing::debug!("PROVER_NETWORK: Received get_filtered_proof_requests request: {:?}. Filtering by fulfillment_status. Not matching: {:?}", request_data, req.fulfillment_status);
                return false;
            }
            
            // Filter by execution status if provided
            if req_inner.execution_status.is_some() && req.execution_status != req_inner.execution_status.unwrap() {
                tracing::debug!("PROVER_NETWORK: Received get_filtered_proof_requests request: {:?}. Filtering by execution_status. Not matching: {:?}", request_data, req.execution_status);
                return false;
            }
            
            // Filter by vk_hash if provided
            if let Some(ref filter_vk_hash) = req_inner.vk_hash {
                if !filter_vk_hash.is_empty() && req.vk_hash != *filter_vk_hash {
                    tracing::debug!("PROVER_NETWORK: Received get_filtered_proof_requests request: {:?}. Filtering by vk_hash. Not matching: {:?}", request_data, req.vk_hash);
                    return false;
                }
            }
            
            // Filter by version if provided
            if let Some(ref filter_version) = req_inner.version {
                if !filter_version.is_empty() && req.version != *filter_version {
                    tracing::debug!("PROVER_NETWORK: Received get_filtered_proof_requests request: {:?}. Filtering by version. Not matching: {:?}", request_data, req.version);
                    return false;
                }
            }
            
            // Filter by mode if provided
            if req_inner.mode.is_some() && req.mode != req_inner.mode.unwrap() {
                tracing::debug!("PROVER_NETWORK: Received get_filtered_proof_requests request: {:?}. Filtering by mode. Not matching: {:?}", request_data, req.mode);
                return false;
            }

            // Filter by minimum_deadline if provided
            if req_inner.minimum_deadline.is_some() && req.deadline <= req_inner.minimum_deadline.unwrap() {
                tracing::debug!("PROVER_NETWORK: Received get_filtered_proof_requests request: {:?}. Filtering by minimum_deadline. Not matching: received in the request {:?} and stored in the proof_request {:?}", request_data, req_inner.minimum_deadline, req.deadline);
                return false;
            }

            // Filter by fulfiller if provided
            if let Some(ref filter_fulfiller) = req_inner.fulfiller {
                if req.fulfiller.as_ref() != Some(filter_fulfiller) {
                    tracing::debug!("PROVER_NETWORK: Received get_filtered_proof_requests request: {:?}. Filtering by fulfiller. Not matching: {:?}", request_data, req.fulfiller);
                    return false;
                }
            }

            // Filter by from if provided
            if req_inner.from.is_some() {
                tracing::debug!("PROVER_NETWORK: Received get_filtered_proof_requests request: {:?}. Filtering by from. Not implemented, ignoring... {:?}", request_data, req_inner.from);
            }

            // Filter by to if provided
            if req_inner.to.is_some() {
                tracing::debug!("PROVER_NETWORK: Received get_filtered_proof_requests request: {:?}. Filtering by to. Not implemented, ignoring... {:?}", request_data, req_inner.to);
            }

            // Filter by not_bid_by if provided
            if req_inner.not_bid_by.is_some() {
                tracing::debug!("PROVER_NETWORK: Received get_filtered_proof_requests request: {:?}. Filtering by not_bid_by. Not implemented, ignoring... {:?}", request_data, req_inner.not_bid_by);
            }

            // Filter by execute_fail_cause if provided
            if req_inner.execute_fail_cause.is_some() && req.execute_fail_cause != req_inner.execute_fail_cause.unwrap() {
                tracing::debug!("PROVER_NETWORK: Received get_filtered_proof_requests request: {:?}. Filtering by execute_fail_cause. Not matching: {:?}", request_data, req.execute_fail_cause);
                return false;
            }

            // Filter by settlement_status if provided
            if req_inner.settlement_status.is_some() && req.settlement_status != req_inner.settlement_status.unwrap() {
                tracing::debug!("PROVER_NETWORK: Received get_filtered_proof_requests request: {:?}. Filtering by settlement_status. Not matching: {:?}", request_data, req.settlement_status);
                return false;
            }

            // Filter by error if provided
            if req_inner.error.is_some() && req.error != req_inner.error.unwrap() {
                tracing::debug!("PROVER_NETWORK: Received get_filtered_proof_requests request: {:?}. Filtering by error. Not matching: {:?}", request_data, req.error);
                return false;
            }

            true
            })
            .collect();
        
        // Sort by created_at in ascending order (oldest first)
        filtered_requests.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        
        // Apply pagination
        let page = req_inner.page.unwrap_or(0) as usize;
        let limit = req_inner.limit.unwrap_or(50) as usize; // Default limit of 50
        let offset = page * limit as usize; // Default page size of 50
        
        // Calculate total count before pagination
        //let total_count = filtered_requests.len();
        
        // Apply offset and limit
        let paginated_requests: Vec<ProofRequest> = filtered_requests
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect();
        
        //tracing::info!("PROVER_NETWORK: Returning {} requests out of {} total", paginated_requests.len(), total_count);
        
        let filtered_requests = paginated_requests;
        
        // Check if this is a request from a specific prover (fulfiller filter) and no results found
        // This indicates the prover is free and available for new assignments
        if let Some(ref prover_address) = req_inner.fulfiller {
            if filtered_requests.is_empty() {
                tracing::trace!("PROVER_ASSIGNMENT: No proof requests found for prover {}, notifying assignment process", hex::encode(prover_address));
                // Notify the background assignment process that this prover is free
                if let Err(e) = self.free_prover_tx.send(prover_address.clone()) {
                    tracing::error!("PROVER_ASSIGNMENT: Failed to notify assignment process: {}", e);
                }
            }
        }
        
        Ok(Response::new(GetFilteredProofRequestsResponse {
            requests: filtered_requests,
        }))
    }

    type SubscribeProofRequestsStream = std::pin::Pin<Box<dyn tokio_stream::Stream<Item = Result<ProofRequest, Status>> + Send>>;

    async fn subscribe_proof_requests(&self, _request: Request<GetFilteredProofRequestsRequest>) -> Result<Response<Self::SubscribeProofRequestsStream>, Status> {
        // // TODO implemente the filtering logic
        // let requests = self.proof_requests.lock().await;
        // let all_requests: Vec<ProofRequest> = requests.values().map(|(req, _)| req.clone()).collect();
        // drop(requests);
        
        // let stream = tokio_stream::iter(all_requests.into_iter().map(Ok));
        // Ok(Response::new(Box::pin(stream)))
        tracing::info!("PROVER_NETWORK: subscribe_proof_requests method called but not implemented");
        Err(Status::unimplemented("subscribe_proof_requests not implemented"))
    }

    async fn get_search_results(&self, _request: Request<GetSearchResultsRequest>) -> Result<Response<GetSearchResultsResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_search_results method called but not implemented");
        Err(Status::unimplemented("get_search_results not implemented"))
    }

    async fn get_proof_request_metrics(&self, _request: Request<GetProofRequestMetricsRequest>) -> Result<Response<GetProofRequestMetricsResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_proof_request_metrics method called but not implemented");
        Err(Status::unimplemented("get_proof_request_metrics not implemented"))
    }

    async fn get_proof_request_graph(&self, _request: Request<GetProofRequestGraphRequest>) -> Result<Response<GetProofRequestGraphResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_proof_request_graph method called but not implemented");
        Err(Status::unimplemented("get_proof_request_graph not implemented"))
    }

    async fn get_analytics_graphs(&self, _request: Request<GetAnalyticsGraphsRequest>) -> Result<Response<GetAnalyticsGraphsResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_analytics_graphs method called but not implemented");
        Err(Status::unimplemented("get_analytics_graphs not implemented"))
    }

    async fn get_overview_graphs(&self, _request: Request<GetOverviewGraphsRequest>) -> Result<Response<GetOverviewGraphsResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_overview_graphs method called but not implemented");
        Err(Status::unimplemented("get_overview_graphs not implemented"))
    }

    async fn get_proof_request_params(&self, _request: Request<GetProofRequestParamsRequest>) -> Result<Response<GetProofRequestParamsResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_proof_request_params method called but not implemented");
        Err(Status::unimplemented("get_proof_request_params not implemented"))
    }

    async fn get_nonce(&self, _request: Request<GetNonceRequest>) -> Result<Response<GetNonceResponse>, Status> {
        Ok(Response::new(GetNonceResponse { nonce: 0 }))
    }

    async fn set_account_name(&self, _request: Request<SetAccountNameRequest>) -> Result<Response<SetAccountNameResponse>, Status> {
        tracing::info!("PROVER_NETWORK: set_account_name method called but not implemented");
        Err(Status::unimplemented("set_account_name not implemented"))
    }

    async fn get_account_name(&self, _request: Request<GetAccountNameRequest>) -> Result<Response<GetAccountNameResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_account_name method called but not implemented");
        Err(Status::unimplemented("get_account_name not implemented"))
    }

    async fn get_terms_signature(&self, _request: Request<GetTermsSignatureRequest>) -> Result<Response<GetTermsSignatureResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_terms_signature method called but not implemented");
        Err(Status::unimplemented("get_terms_signature not implemented"))
    }

    async fn set_terms_signature(&self, _request: Request<SetTermsSignatureRequest>) -> Result<Response<SetTermsSignatureResponse>, Status> {
        tracing::info!("PROVER_NETWORK: set_terms_signature method called but not implemented");
        Err(Status::unimplemented("set_terms_signature not implemented"))
    }

    async fn get_account(&self, _request: Request<GetAccountRequest>) -> Result<Response<GetAccountResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_account method called but not implemented");
        Err(Status::unimplemented("get_account not implemented"))
    }

    async fn get_owner(&self, _request: Request<GetOwnerRequest>) -> Result<Response<GetOwnerResponse>, Status> {
        let acct = _request.into_inner().address;
        Ok(Response::new(GetOwnerResponse { owner: acct.clone() }))
    }

    async fn get_program(&self, _request: Request<GetProgramRequest>) -> Result<Response<GetProgramResponse>, Status> {
        let request_inner = _request.into_inner();
        tracing::info!("PROVER_NETWORK: Received get_program request: {:?}", hex::encode(&request_inner.vk_hash));
        
        if let Some(program) = self.get_program_by_vk_hash(&request_inner.vk_hash)? {            
            let response = GetProgramResponse {
                program: Some(program),
            };
            Ok(Response::new(response))
        } else {
            Err(Status::not_found("Program not found"))
        }
    }

    async fn create_program(&self, _request: Request<CreateProgramRequest>) -> Result<Response<CreateProgramResponse>, Status> {
        let request_inner = _request.into_inner();
        let body: CreateProgramRequestBody = request_inner.body.ok_or_else(|| Status::invalid_argument("Request body is required"))?;
        let msg_bytes: Vec<u8> = encode_body_for_signing(request_inner.format, &body)
            .map_err(|e| Status::internal(format!("Failed to encode body for signing: {}", e)))?;
        let requester = recover_signer_addr(msg_bytes, &request_inner.signature)
            .map_err(|e| Status::invalid_argument(format!("Failed to recover signer address: {}", e)))?;
        let vk_hash_key = body.vk_hash.clone();
        let program = rpc_types::Program {
            vk_hash: body.vk_hash,
            vk: body.vk,
            program_uri: body.program_uri,
            name: None,
            owner: requester.clone(),
            created_at: chrono::Utc::now().timestamp() as u64,
        };
        
        // Store the program in RocksDB
        self.store_program(&vk_hash_key, &program)?;

        let response = CreateProgramResponse {
            tx_hash: random::<[u8; 32]>().to_vec(),
            body: Some(CreateProgramResponseBody {})
        };
        Ok(Response::new(response))
    }

    async fn set_program_name(&self, _request: Request<SetProgramNameRequest>) -> Result<Response<SetProgramNameResponse>, Status> {
        tracing::info!("PROVER_NETWORK: set_program_name method called but not implemented");
        Err(Status::unimplemented("set_program_name not implemented"))
    }

    async fn get_balance(&self, _request: Request<GetBalanceRequest>) -> Result<Response<GetBalanceResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_balance method called but not implemented");
        Err(Status::unimplemented("get_balance not implemented"))
    }

    async fn get_filtered_balance_logs(&self, _request: Request<GetFilteredBalanceLogsRequest>) -> Result<Response<GetFilteredBalanceLogsResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_filtered_balance_logs method called but not implemented");
        Err(Status::unimplemented("get_filtered_balance_logs not implemented"))
    }

    async fn add_credit(&self, _request: Request<AddCreditRequest>) -> Result<Response<AddCreditResponse>, Status> {
        tracing::info!("PROVER_NETWORK: add_credit method called but not implemented");
        Err(Status::unimplemented("add_credit not implemented"))
    }

    async fn get_latest_bridge_block(&self, _request: Request<GetLatestBridgeBlockRequest>) -> Result<Response<GetLatestBridgeBlockResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_latest_bridge_block method called but not implemented");
        Err(Status::unimplemented("get_latest_bridge_block not implemented"))
    }

    async fn get_gas_price_estimate(&self, _request: Request<GetGasPriceEstimateRequest>) -> Result<Response<GetGasPriceEstimateResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_gas_price_estimate method called but not implemented");
        Err(Status::unimplemented("get_gas_price_estimate not implemented"))
    }

    async fn get_transaction_details(&self, _request: Request<GetTransactionDetailsRequest>) -> Result<Response<GetTransactionDetailsResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_transaction_details method called but not implemented");
        Err(Status::unimplemented("get_transaction_details not implemented"))
    }

    async fn add_reserved_charge(&self, _request: Request<AddReservedChargeRequest>) -> Result<Response<AddReservedChargeResponse>, Status> {
        tracing::info!("PROVER_NETWORK: add_reserved_charge method called but not implemented");
        Err(Status::unimplemented("add_reserved_charge not implemented"))
    }

    async fn get_billing_summary(&self, _request: Request<GetBillingSummaryRequest>) -> Result<Response<GetBillingSummaryResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_billing_summary method called but not implemented");
        Err(Status::unimplemented("get_billing_summary not implemented"))
    }

    async fn update_price(&self, _request: Request<UpdatePriceRequest>) -> Result<Response<UpdatePriceResponse>, Status> {
        tracing::info!("PROVER_NETWORK: update_price method called but not implemented");
        Err(Status::unimplemented("update_price not implemented"))
    }

    async fn get_filtered_clusters(&self, _request: Request<GetFilteredClustersRequest>) -> Result<Response<GetFilteredClustersResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_filtered_clusters method called but not implemented");
        Err(Status::unimplemented("get_filtered_clusters not implemented"))
    }

    async fn get_usage_summary(&self, _request: Request<GetUsageSummaryRequest>) -> Result<Response<GetUsageSummaryResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_usage_summary method called but not implemented");
        Err(Status::unimplemented("get_usage_summary not implemented"))
    }

    async fn transfer(&self, _request: Request<TransferRequest>) -> Result<Response<TransferResponse>, Status> {
        tracing::info!("PROVER_NETWORK: transfer method called but not implemented");
        Err(Status::unimplemented("transfer not implemented"))
    }

    async fn get_withdraw_params(&self, _request: Request<GetWithdrawParamsRequest>) -> Result<Response<GetWithdrawParamsResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_withdraw_params method called but not implemented");
        Err(Status::unimplemented("get_withdraw_params not implemented"))
    }

    async fn withdraw(&self, _request: Request<rpc_types::WithdrawRequest>) -> Result<Response<WithdrawResponse>, Status> {
        tracing::info!("PROVER_NETWORK: withdraw method called but not implemented");
        Err(Status::unimplemented("withdraw not implemented"))
    }

    async fn get_filtered_reservations(&self, _request: Request<GetFilteredReservationsRequest>) -> Result<Response<GetFilteredReservationsResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_filtered_reservations method called but not implemented");
        Err(Status::unimplemented("get_filtered_reservations not implemented"))
    }

    async fn add_reservation(&self, _request: Request<AddReservationRequest>) -> Result<Response<AddReservationResponse>, Status> {
        tracing::info!("PROVER_NETWORK: add_reservation method called but not implemented");
        Err(Status::unimplemented("add_reservation not implemented"))
    }

    async fn remove_reservation(&self, _request: Request<RemoveReservationRequest>) -> Result<Response<RemoveReservationResponse>, Status> {
        tracing::info!("PROVER_NETWORK: remove_reservation method called but not implemented");
        Err(Status::unimplemented("remove_reservation not implemented"))
    }

    async fn bid(&self, _request: Request<BidRequest>) -> Result<Response<BidResponse>, Status> {
        tracing::info!("PROVER_NETWORK: bid method called but not implemented");
        Err(Status::unimplemented("bid not implemented"))
    }

    async fn settle(&self, _request: Request<SettleRequest>) -> Result<Response<SettleResponse>, Status> {
        tracing::info!("PROVER_NETWORK: settle method called but not implemented");
        Err(Status::unimplemented("settle not implemented"))
    }

    async fn get_provers_by_uptime(&self, _request: Request<GetProversByUptimeRequest>) -> Result<Response<GetProversByUptimeResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_provers_by_uptime method called but not implemented");
        Err(Status::unimplemented("get_provers_by_uptime not implemented"))
    }

    async fn sign_in(&self, _request: Request<SignInRequest>) -> Result<Response<SignInResponse>, Status> {
        tracing::info!("PROVER_NETWORK: sign_in method called but not implemented");
        Err(Status::unimplemented("sign_in not implemented"))
    }

    async fn get_onboarded_accounts_count(&self, _request: Request<GetOnboardedAccountsCountRequest>) -> Result<Response<GetOnboardedAccountsCountResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_onboarded_accounts_count method called but not implemented");
        Err(Status::unimplemented("get_onboarded_accounts_count not implemented"))
    }

    async fn get_filtered_onboarded_accounts(&self, _request: Request<GetFilteredOnboardedAccountsRequest>) -> Result<Response<GetFilteredOnboardedAccountsResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_filtered_onboarded_accounts method called but not implemented");
        Err(Status::unimplemented("get_filtered_onboarded_accounts not implemented"))
    }

    async fn get_leaderboard(&self, _request: Request<GetLeaderboardRequest>) -> Result<Response<GetLeaderboardResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_leaderboard method called but not implemented");
        Err(Status::unimplemented("get_leaderboard not implemented"))
    }

    async fn get_leaderboard_stats(&self, _request: Request<GetLeaderboardStatsRequest>) -> Result<Response<GetLeaderboardStatsResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_leaderboard_stats method called but not implemented");
        Err(Status::unimplemented("get_leaderboard_stats not implemented"))
    }

    async fn get_codes(&self, _request: Request<GetCodesRequest>) -> Result<Response<GetCodesResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_codes method called but not implemented");
        Err(Status::unimplemented("get_codes not implemented"))
    }

    async fn redeem_code(&self, _request: Request<RedeemCodeRequest>) -> Result<Response<RedeemCodeResponse>, Status> {
        tracing::info!("PROVER_NETWORK: redeem_code method called but not implemented");
        Err(Status::unimplemented("redeem_code not implemented"))
    }

    async fn connect_twitter(&self, _request: Request<ConnectTwitterRequest>) -> Result<Response<ConnectTwitterResponse>, Status> {
        tracing::info!("PROVER_NETWORK: connect_twitter method called but not implemented");
        Err(Status::unimplemented("connect_twitter not implemented"))
    }

    async fn complete_onboarding(&self, _request: Request<CompleteOnboardingRequest>) -> Result<Response<CompleteOnboardingResponse>, Status> {
        tracing::info!("PROVER_NETWORK: complete_onboarding method called but not implemented");
        Err(Status::unimplemented("complete_onboarding not implemented"))
    }

    async fn set_use_twitter_handle(&self, _request: Request<SetUseTwitterHandleRequest>) -> Result<Response<SetUseTwitterHandleResponse>, Status> {
        tracing::info!("PROVER_NETWORK: set_use_twitter_handle method called but not implemented");
        Err(Status::unimplemented("set_use_twitter_handle not implemented"))
    }

    async fn set_use_twitter_image(&self, _request: Request<SetUseTwitterImageRequest>) -> Result<Response<SetUseTwitterImageResponse>, Status> {
        tracing::info!("PROVER_NETWORK: set_use_twitter_image method called but not implemented");
        Err(Status::unimplemented("set_use_twitter_image not implemented"))
    }

    async fn request_random_proof(&self, _request: Request<RequestRandomProofRequest>) -> Result<Response<RequestRandomProofResponse>, Status> {
        tracing::info!("PROVER_NETWORK: request_random_proof method called but not implemented");
        Err(Status::unimplemented("request_random_proof not implemented"))
    }

    async fn submit_captcha_game(&self, _request: Request<SubmitCaptchaGameRequest>) -> Result<Response<SubmitCaptchaGameResponse>, Status> {
        tracing::info!("PROVER_NETWORK: submit_captcha_game method called but not implemented");
        Err(Status::unimplemented("submit_captcha_game not implemented"))
    }

    async fn redeem_stars(&self, _request: Request<RedeemStarsRequest>) -> Result<Response<RedeemStarsResponse>, Status> {
        tracing::info!("PROVER_NETWORK: redeem_stars method called but not implemented");
        Err(Status::unimplemented("redeem_stars not implemented"))
    }

    async fn get_flappy_leaderboard(&self, _request: Request<GetFlappyLeaderboardRequest>) -> Result<Response<GetFlappyLeaderboardResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_flappy_leaderboard method called but not implemented");
        Err(Status::unimplemented("get_flappy_leaderboard not implemented"))
    }

    async fn set_turbo_high_score(&self, _request: Request<SetTurboHighScoreRequest>) -> Result<Response<SetTurboHighScoreResponse>, Status> {
        tracing::info!("PROVER_NETWORK: set_turbo_high_score method called but not implemented");
        Err(Status::unimplemented("set_turbo_high_score not implemented"))
    }

    async fn submit_quiz_game(&self, _request: Request<SubmitQuizGameRequest>) -> Result<Response<SubmitQuizGameResponse>, Status> {
        tracing::info!("PROVER_NETWORK: submit_quiz_game method called but not implemented");
        Err(Status::unimplemented("submit_quiz_game not implemented"))
    }

    async fn get_turbo_leaderboard(&self, _request: Request<GetTurboLeaderboardRequest>) -> Result<Response<GetTurboLeaderboardResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_turbo_leaderboard method called but not implemented");
        Err(Status::unimplemented("get_turbo_leaderboard not implemented"))
    }

    async fn submit_eth_block_metadata(&self, _request: Request<SubmitEthBlockMetadataRequest>) -> Result<Response<SubmitEthBlockMetadataResponse>, Status> {
        tracing::info!("PROVER_NETWORK: submit_eth_block_metadata method called but not implemented");
        Err(Status::unimplemented("submit_eth_block_metadata not implemented"))
    }

    async fn get_filtered_eth_block_requests(&self, _request: Request<GetFilteredEthBlockRequestsRequest>) -> Result<Response<GetFilteredEthBlockRequestsResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_filtered_eth_block_requests method called but not implemented");
        Err(Status::unimplemented("get_filtered_eth_block_requests not implemented"))
    }

    async fn set2048_high_score(&self, _request: Request<Set2048HighScoreRequest>) -> Result<Response<Set2048HighScoreResponse>, Status> {
        tracing::info!("PROVER_NETWORK: set2048_high_score method called but not implemented");
        Err(Status::unimplemented("set2048_high_score not implemented"))
    }

    async fn set_volleyball_high_score(&self, _request: Request<SetVolleyballHighScoreRequest>) -> Result<Response<SetVolleyballHighScoreResponse>, Status> {
        tracing::info!("PROVER_NETWORK: set_volleyball_high_score method called but not implemented");
        Err(Status::unimplemented("set_volleyball_high_score not implemented"))
    }

    async fn get_eth_block_request_metrics(&self, _request: Request<GetEthBlockRequestMetricsRequest>) -> Result<Response<GetEthBlockRequestMetricsResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_eth_block_request_metrics method called but not implemented");
        Err(Status::unimplemented("get_eth_block_request_metrics not implemented"))
    }

    async fn set_turbo_time_trial_high_score(&self, _request: Request<SetTurboTimeTrialHighScoreRequest>) -> Result<Response<SetTurboTimeTrialHighScoreResponse>, Status> {
        tracing::info!("PROVER_NETWORK: set_turbo_time_trial_high_score method called but not implemented");
        Err(Status::unimplemented("set_turbo_time_trial_high_score not implemented"))
    }

    async fn set_coin_craze_high_score(&self, _request: Request<SetCoinCrazeHighScoreRequest>) -> Result<Response<SetCoinCrazeHighScoreResponse>, Status> {
        tracing::info!("PROVER_NETWORK: set_coin_craze_high_score method called but not implemented");
        Err(Status::unimplemented("set_coin_craze_high_score not implemented"))
    }

    async fn set_lean_high_score(&self, _request: Request<SetLeanHighScoreRequest>) -> Result<Response<SetLeanHighScoreResponse>, Status> {
        tracing::info!("PROVER_NETWORK: set_lean_high_score method called but not implemented");
        Err(Status::unimplemented("set_lean_high_score not implemented"))
    }

    async fn set_flow_high_score(&self, _request: Request<SetFlowHighScoreRequest>) -> Result<Response<SetFlowHighScoreResponse>, Status> {
        tracing::info!("PROVER_NETWORK: set_flow_high_score method called but not implemented");
        Err(Status::unimplemented("set_flow_high_score not implemented"))
    }

    async fn set_rollup_high_score(&self, _request: Request<SetRollupHighScoreRequest>) -> Result<Response<SetRollupHighScoreResponse>, Status> {
        tracing::info!("PROVER_NETWORK: set_rollup_high_score method called but not implemented");
        Err(Status::unimplemented("set_rollup_high_score not implemented"))
    }

    async fn get_pending_stars(&self, _request: Request<GetPendingStarsRequest>) -> Result<Response<GetPendingStarsResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_pending_stars method called but not implemented");
        Err(Status::unimplemented("get_pending_stars not implemented"))
    }

    async fn get_whitelist_status(&self, _request: Request<GetWhitelistStatusRequest>) -> Result<Response<GetWhitelistStatusResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_whitelist_status method called but not implemented");
        Err(Status::unimplemented("get_whitelist_status not implemented"))
    }

    async fn claim_gpu(&self, _request: Request<ClaimGpuRequest>) -> Result<Response<ClaimGpuResponse>, Status> {
        tracing::info!("PROVER_NETWORK: claim_gpu method called but not implemented");
        Err(Status::unimplemented("claim_gpu not implemented"))
    }

    async fn set_gpu_variant(&self, _request: Request<SetGpuVariantRequest>) -> Result<Response<SetGpuVariantResponse>, Status> {
        tracing::info!("PROVER_NETWORK: set_gpu_variant method called but not implemented");
        Err(Status::unimplemented("set_gpu_variant not implemented"))
    }

    async fn link_whitelisted_twitter(&self, _request: Request<LinkWhitelistedTwitterRequest>) -> Result<Response<LinkWhitelistedTwitterResponse>, Status> {
        tracing::info!("PROVER_NETWORK: link_whitelisted_twitter method called but not implemented");
        Err(Status::unimplemented("link_whitelisted_twitter not implemented"))
    }

    async fn retrieve_proving_key(&self, _request: Request<RetrieveProvingKeyRequest>) -> Result<Response<RetrieveProvingKeyResponse>, Status> {
        tracing::info!("PROVER_NETWORK: retrieve_proving_key method called but not implemented");
        Err(Status::unimplemented("retrieve_proving_key not implemented"))
    }

    async fn link_whitelisted_github(&self, _request: Request<LinkWhitelistedGithubRequest>) -> Result<Response<LinkWhitelistedGithubResponse>, Status> {
        tracing::info!("PROVER_NETWORK: link_whitelisted_github method called but not implemented");
        Err(Status::unimplemented("link_whitelisted_github not implemented"))
    }

    async fn link_whitelisted_discord(&self, _request: Request<LinkWhitelistedDiscordRequest>) -> Result<Response<LinkWhitelistedDiscordResponse>, Status> {
        tracing::info!("PROVER_NETWORK: link_whitelisted_discord method called but not implemented");
        Err(Status::unimplemented("link_whitelisted_discord not implemented"))
    }

    async fn get_prover_leaderboard(&self, _request: Request<GetProverLeaderboardRequest>) -> Result<Response<GetProverLeaderboardResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_prover_leaderboard method called but not implemented");
        Err(Status::unimplemented("get_prover_leaderboard not implemented"))
    }

    async fn get_filtered_gpus(&self, _request: Request<GetFilteredGpusRequest>) -> Result<Response<GetFilteredGpusResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_filtered_gpus method called but not implemented");
        Err(Status::unimplemented("get_filtered_gpus not implemented"))
    }

    async fn set_gpu_coordinates(&self, _request: Request<SetGpuCoordinatesRequest>) -> Result<Response<SetGpuCoordinatesResponse>, Status> {
        tracing::info!("PROVER_NETWORK: set_gpu_coordinates method called but not implemented");
        Err(Status::unimplemented("set_gpu_coordinates not implemented"))
    }

    async fn get_points(&self, _request: Request<GetPointsRequest>) -> Result<Response<GetPointsResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_points method called but not implemented");
        Err(Status::unimplemented("get_points not implemented"))
    }

    async fn process_clicks(&self, _request: Request<ProcessClicksRequest>) -> Result<Response<ProcessClicksResponse>, Status> {
        tracing::info!("PROVER_NETWORK: process_clicks method called but not implemented");
        Err(Status::unimplemented("process_clicks not implemented"))
    }

    async fn purchase_upgrade(&self, _request: Request<PurchaseUpgradeRequest>) -> Result<Response<PurchaseUpgradeResponse>, Status> {
        tracing::info!("PROVER_NETWORK: purchase_upgrade method called but not implemented");
        Err(Status::unimplemented("purchase_upgrade not implemented"))
    }

    async fn bet(&self, _request: Request<BetRequest>) -> Result<Response<BetResponse>, Status> {
        tracing::info!("PROVER_NETWORK: bet method called but not implemented");
        Err(Status::unimplemented("bet not implemented"))
    }

    async fn get_contest_details(&self, _request: Request<GetContestDetailsRequest>) -> Result<Response<GetContestDetailsResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_contest_details method called but not implemented");
        Err(Status::unimplemented("get_contest_details not implemented"))
    }

    async fn get_latest_contest(&self, _request: Request<GetLatestContestRequest>) -> Result<Response<GetLatestContestResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_latest_contest method called but not implemented");
        Err(Status::unimplemented("get_latest_contest not implemented"))
    }

    async fn get_contest_bettors(&self, _request: Request<GetContestBettorsRequest>) -> Result<Response<GetContestBettorsResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_contest_bettors method called but not implemented");
        Err(Status::unimplemented("get_contest_bettors not implemented"))
    }

    async fn get_gpu_metrics(&self, _request: Request<GetGpuMetricsRequest>) -> Result<Response<GetGpuMetricsResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_gpu_metrics method called but not implemented");
        Err(Status::unimplemented("get_gpu_metrics not implemented"))
    }

    async fn get_filtered_prover_activity(&self, _request: Request<GetFilteredProverActivityRequest>) -> Result<Response<GetFilteredProverActivityResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_filtered_prover_activity method called but not implemented");
        Err(Status::unimplemented("get_filtered_prover_activity not implemented"))
    }

    async fn get_prover_metrics(&self, _request: Request<GetProverMetricsRequest>) -> Result<Response<GetProverMetricsResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_prover_metrics method called but not implemented");
        Err(Status::unimplemented("get_prover_metrics not implemented"))
    }

    async fn get_filtered_bet_history(&self, _request: Request<GetFilteredBetHistoryRequest>) -> Result<Response<GetFilteredBetHistoryResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_filtered_bet_history method called but not implemented");
        Err(Status::unimplemented("get_filtered_bet_history not implemented"))
    }

    async fn get_gpu_team_stats(&self, _request: Request<GetGpuTeamStatsRequest>) -> Result<Response<GetGpuTeamStatsResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_gpu_team_stats method called but not implemented");
        Err(Status::unimplemented("get_gpu_team_stats not implemented"))
    }

    async fn get_config_values(&self, _request: Request<GetConfigValuesRequest>) -> Result<Response<GetConfigValuesResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_config_values method called but not implemented");
        Err(Status::unimplemented("get_config_values not implemented"))
    }

    async fn get_prover_stats(&self, _request: Request<GetProverStatsRequest>) -> Result<Response<GetProverStatsResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_prover_stats method called but not implemented");
        Err(Status::unimplemented("get_prover_stats not implemented"))
    }

    async fn get_filtered_prover_stats(&self, _request: Request<GetFilteredProverStatsRequest>) -> Result<Response<GetFilteredProverStatsResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_filtered_prover_stats method called but not implemented");
        Err(Status::unimplemented("get_filtered_prover_stats not implemented"))
    }

    async fn get_prover_stats_detail(&self, _request: Request<GetProverStatsDetailRequest>) -> Result<Response<GetProverStatsDetailResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_prover_stats_detail method called but not implemented");
        Err(Status::unimplemented("get_prover_stats_detail not implemented"))
    }

    async fn get_prover_search_results(&self, _request: Request<GetProverSearchResultsRequest>) -> Result<Response<GetProverSearchResultsResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_prover_search_results method called but not implemented");
        Err(Status::unimplemented("get_prover_search_results not implemented"))
    }

    async fn get_filtered_bid_history(&self, _request: Request<GetFilteredBidHistoryRequest>) -> Result<Response<GetFilteredBidHistoryResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_filtered_bid_history method called but not implemented");
        Err(Status::unimplemented("get_filtered_bid_history not implemented"))
    }

    async fn get_tee_whitelist_status(&self, _request: Request<GetTeeWhitelistStatusRequest>) -> Result<Response<GetTeeWhitelistStatusResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_tee_whitelist_status method called but not implemented");
        Err(Status::unimplemented("get_tee_whitelist_status not implemented"))
    }

    async fn get_settlement_request(&self, _request: Request<GetSettlementRequestRequest>) -> Result<Response<GetSettlementRequestResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_settlement_request method called but not implemented");
        Err(Status::unimplemented("get_settlement_request not implemented"))
    }

    async fn get_filtered_settlement_requests(&self, _request: Request<GetFilteredSettlementRequestsRequest>) -> Result<Response<GetFilteredSettlementRequestsResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_filtered_settlement_requests method called but not implemented");
        Err(Status::unimplemented("get_filtered_settlement_requests not implemented"))
    }

    async fn get_filtered_provers(&self, _request: Request<GetFilteredProversRequest>) -> Result<Response<GetFilteredProversResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_filtered_provers method called but not implemented");
        Err(Status::unimplemented("get_filtered_provers not implemented"))
    }

    async fn get_prover_stake_balance(&self, _request: Request<GetProverStakeBalanceRequest>) -> Result<Response<GetProverStakeBalanceResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_prover_stake_balance method called but not implemented");
        Err(Status::unimplemented("get_prover_stake_balance not implemented"))
    }

    async fn get_filtered_staker_stake_balance_logs(&self, _request: Request<GetFilteredStakerStakeBalanceLogsRequest>) -> Result<Response<GetFilteredStakerStakeBalanceLogsResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_filtered_staker_stake_balance_logs method called but not implemented");
        Err(Status::unimplemented("get_filtered_staker_stake_balance_logs not implemented"))
    }

    async fn get_filtered_prover_stake_balance_logs(&self, _request: Request<GetFilteredProverStakeBalanceLogsRequest>) -> Result<Response<GetFilteredProverStakeBalanceLogsResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_filtered_prover_stake_balance_logs method called but not implemented");
        Err(Status::unimplemented("get_filtered_prover_stake_balance_logs not implemented"))
    }

    async fn get_delegation_params(&self, _request: Request<GetDelegationParamsRequest>) -> Result<Response<GetDelegationParamsResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_delegation_params method called but not implemented");
        Err(Status::unimplemented("get_delegation_params not implemented"))
    }

    async fn set_delegation(&self, _request: Request<SetDelegationRequest>) -> Result<Response<SetDelegationResponse>, Status> {
        tracing::info!("PROVER_NETWORK: set_delegation method called but not implemented");
        Err(Status::unimplemented("set_delegation not implemented"))
    }

    async fn get_delegation(&self, _request: Request<GetDelegationRequest>) -> Result<Response<GetDelegationResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_delegation method called but not implemented");
        Err(Status::unimplemented("get_delegation not implemented"))
    }

    async fn get_filtered_withdrawal_receipts(&self, _request: Request<GetFilteredWithdrawalReceiptsRequest>) -> Result<Response<GetFilteredWithdrawalReceiptsResponse>, Status> {
        tracing::info!("PROVER_NETWORK: get_filtered_withdrawal_receipts method called but not implemented");
        Err(Status::unimplemented("get_filtered_withdrawal_receipts not implemented"))
    }
}

fn encode_body_for_signing<T: Message>(format: i32, body: &T) -> eyre::Result<Vec<u8>> {
    let fmt = MessageFormat::try_from(format).unwrap_or(MessageFormat::Binary);
    match fmt {
        MessageFormat::Binary => {
            // Protobuf canonical binary
            let mut buf = Vec::new();
            body.encode(&mut buf)?;
            Ok(buf)
        }
        // MessageFormat::Json => {
        //     // Only use if your client truly signed JSON and both sides enforce a canonical form.
        //     // If you control both ends, prefer Binary to avoid JSON canonicalization traps.
        //     #[derive(Serialize)]
        //     struct Canon<'a> {
        //         nonce: u64,
        //         vk_hash: &'a [u8],
        //         version: &'a str,
        //         mode: i32,
        //         strategy: i32,
        //         stdin_uri: &'a str,
        //         deadline: u64,
        //         cycle_limit: u64,
        //         gas_limit: u64,
        //     }
        //     // Map your fields EXACTLY as the client did:
        //     let c = Canon {
        //         nonce: body.nonce,
        //         vk_hash: &body.vk_hash,
        //         version: &body.version,
        //         mode: body.mode,
        //         strategy: body.strategy,
        //         stdin_uri: &body.stdin_uri,
        //         deadline: body.deadline,
        //         cycle_limit: body.cycle_limit,
        //         gas_limit: body.gas_limit,
        //     };
        //     Ok(serde_json::to_vec(&c)?)
        // }
        // Fallbacks if your enum has others:
        _ => {
            // Default to protobuf binary unless you KNOW another format was used.
            let mut buf = Vec::new();
            body.encode(&mut buf)?;
            Ok(buf)
        }
    }
}

pub fn recover_signer_addr(msg_bytes: Vec<u8>, sig_bytes: &[u8]) -> eyre::Result<Vec<u8>> {
    // Apply EIP-191 prefix (Ethereum personal message format)
    let msg_hash = hash_message(&msg_bytes); // This applies EIP-191 prefix and hashes

    // Parse the signature and recover the address
    let sig = Signature::try_from(sig_bytes)?;
    let address = sig.recover(msg_hash)?;
    let address_bytes = address.as_bytes().to_vec();
    Ok(address_bytes)
}

// pub fn recover_address_from_personal_sign(message: impl AsRef<[u8]>, sig_hex: &str) -> Result<Address> {
//     // Parse 0x… signature; v can be 27/28 or 0/1 — ethers handles both.
//     let sig = Signature::from_str(sig_hex)?;
//     // Keccak256("\x19Ethereum Signed Message:\n{len(m)}" || m)
//     let digest = hash_message(message);
//     // Recover the address that signed the digest
//     let addr = sig.recover(digest)?;
//     Ok(addr)
// }

fn generate_proof_url(artifact_base_url: &str) -> String {
    // Generate a URL pointing to our HTTP server
    // The client will use this URL to PUT the artifact data
    format!("{}/artifacts/Proof/{}", artifact_base_url, hex::encode(random::<[u8; 16]>()))
}

// Static method for background task (since it can't access self)
// Uses mutex to prevent race conditions during assignment
async fn assign_proof_to_prover_static(db: &Arc<DB>, prover_address: &[u8], assignment_mutex: &Arc<Mutex<()>>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Acquire the mutex lock to ensure only one assignment process runs at a time
    let _lock = assignment_mutex.lock().await;
    
    tracing::debug!(
        "PROVER_ASSIGNMENT: Starting assignment process for prover {} (static method with passed mutex)",
        hex::encode(prover_address)
    );
    
    let table_name = "proof_requests";
    let column_family_handle = db.cf_handle(table_name)
        .ok_or("Column family 'proof_requests' not found")?;
    
    // Find the oldest unassigned proof request
    let iter = db.iterator_cf(&column_family_handle, rocksdb::IteratorMode::Start);
    
    for item in iter {
        let (key, value) = item?;
        let (mut proof_request, mut status_response): (ProofRequest, GetProofRequestStatusResponse) = 
            bincode::deserialize(&value)?;
        
        // Check if this proof request is unassigned
        if proof_request.fulfillment_status == FulfillmentStatus::Requested as i32 {
            // Assign the proof request to this prover
            proof_request.fulfillment_status = FulfillmentStatus::Assigned as i32;
            proof_request.fulfiller = Some(prover_address.to_vec());
            proof_request.updated_at = chrono::Utc::now().timestamp() as u64;
            
            // Update the status response
            status_response.fulfillment_status = FulfillmentStatus::Assigned as i32;
            
            // Serialize and store the updated proof request
            let updated_data = (proof_request.clone(), status_response);
            let serialized_data = bincode::serialize(&updated_data)?;
            db.put_cf(&column_family_handle, &key, serialized_data)?;
            
            tracing::info!(
                "PROVER_ASSIGNMENT: Successfully assigned proof request {} to prover {} (static method with passed mutex)",
                hex::encode(&key),
                hex::encode(prover_address)
            );
            
            // Only assign one proof per call
            return Ok(());
        }
    }
    
    tracing::debug!(
        "PROVER_ASSIGNMENT: No unassigned proof requests found for prover {} (static method with passed mutex)",
        hex::encode(prover_address)
    );
    
    Ok(())
    // Mutex lock is automatically released when _lock goes out of scope
}

// Function to check for expired proof assignments and reassign them
// 
// This function implements a timeout mechanism to handle cases where:
// 1. A prover is assigned a proof request but goes offline
// 2. A prover takes too long to complete the proof
// 
// Timeout Logic:
// - Timeout = deadline/2 (half the time until the proof deadline)
// - Minimum timeout: 5 minutes (300 seconds)
// - Maximum timeout: 2 hours (7200 seconds)
// - Uses proof_request.updated_at as the assignment timestamp
// 
// When a timeout occurs:
// - The proof request is marked as "Requested" again (unassigned)
// - The fulfiller field is cleared
// - The proof becomes available for reassignment to other provers
async fn check_and_reassign_expired_proofs(db: &Arc<DB>, assignment_mutex: &Arc<Mutex<()>>, config_timeout: u64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Acquire the mutex lock to ensure no conflicts with assignment process
    let _lock = assignment_mutex.lock().await;
    
    tracing::trace!("TIMEOUT_CHECKER: Starting timeout check for assigned proof requests");
    
    let table_name = "proof_requests";
    let column_family_handle = db.cf_handle(table_name)
        .ok_or("Column family 'proof_requests' not found")?;
    
    let current_time = chrono::Utc::now().timestamp() as u64;
    let iter = db.iterator_cf(&column_family_handle, rocksdb::IteratorMode::Start);
    let mut expired_count = 0;
    
    for item in iter {
        let (key, value) = item?;
        let (mut proof_request, mut status_response): (ProofRequest, GetProofRequestStatusResponse) = 
            bincode::deserialize(&value)?;
        
        // Check if this proof request is assigned and potentially expired
        if proof_request.fulfillment_status == FulfillmentStatus::Assigned as i32 {
            // Calculate timeout: deadline/2, with minimum of 5 minutes and maximum of 2 hours
            let deadline_duration = proof_request.deadline.saturating_sub(proof_request.created_at);
            let timeout_duration = if config_timeout > 0 {
                tracing::trace!("Using configured timeout of {} seconds for proof request {}", config_timeout, hex::encode(&key));
                config_timeout
            } else {
                std::cmp::max(
                    300,  // minimum 5 minutes
                    std::cmp::min(
                        7200, // maximum 2 hours  
                        deadline_duration / 2
                    )
                )
            };
            
            let assignment_time = proof_request.updated_at; // This is when it was assigned
            let timeout_threshold = assignment_time + timeout_duration;

            if current_time > timeout_threshold {
                tracing::warn!(
                    "TIMEOUT_CHECKER: Proof request {} assigned to prover {} has expired (assigned at {}, timeout at {}, current time {})",
                    hex::encode(&key),
                    proof_request.fulfiller.as_ref().map(|f| hex::encode(f)).unwrap_or_default(),
                    assignment_time,
                    timeout_threshold,
                    current_time
                );
                
                // Reassign the proof request back to unassigned status
                proof_request.fulfillment_status = FulfillmentStatus::Requested as i32;
                proof_request.fulfiller = None;
                proof_request.updated_at = current_time;
                
                // Update the status response
                status_response.fulfillment_status = FulfillmentStatus::Requested as i32;
                
                // Serialize and store the updated proof request
                let updated_data = (proof_request.clone(), status_response);
                let serialized_data = bincode::serialize(&updated_data)?;
                db.put_cf(&column_family_handle, &key, serialized_data)?;
                
                expired_count += 1;
                
                tracing::info!(
                    "TIMEOUT_CHECKER: Successfully reassigned expired proof request {} back to unassigned status",
                    hex::encode(&key)
                );
            }
        }
    }
    
    if expired_count > 0 {
        tracing::info!("TIMEOUT_CHECKER: Reassigned {} expired proof assignments", expired_count);
    } else {
        tracing::trace!("TIMEOUT_CHECKER: No expired proof assignments found");
    }
    
    Ok(())
    // Mutex lock is automatically released when _lock goes out of scope
}