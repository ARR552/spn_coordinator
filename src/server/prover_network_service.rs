use anyhow::Result;
use rpc_types::*;
use std::collections::HashMap;
use tokio::sync::Mutex;
use tonic::{Request, Response, Status};
use ethers_core::types::{Signature};
use ethers_core::utils::hash_message; // adds the EIP-191 prefix
use eyre;
use rand::random;
use prost::Message;

/// Real gRPC service implementation for ProverNetwork
#[derive(Debug, Default)]
pub struct ProverNetworkServiceImpl {
    /// TODO Store proof requests in memory (in real implementation this would be a database)  
    proof_requests: Mutex<HashMap<Vec<u8>, (ProofRequest, GetProofRequestStatusResponse)>>,
    programs: Mutex<HashMap<Vec<u8>, Program>>,
}

#[tonic::async_trait]
impl prover_network_server::ProverNetwork for ProverNetworkServiceImpl {
    async fn request_proof(
        &self,
        request: Request<RequestProofRequest>,
    ) -> Result<Response<RequestProofResponse>, Status> {
        let req = request.into_inner();
        println!("PROVER_NETWORK: Server Request params: {:?}", req);
        println!("PROVER_NETWORK: Server Signature received: {:?}", hex::encode(&req.signature));
        
        // Generate a unique request ID
        let request_id = random::<[u8; 32]>().to_vec();
        println!("PROVER_NETWORK: Server Request_id: {:?}", hex::encode(&request_id));
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
            fulfillment_status: FulfillmentStatus::Assigned as i32,
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
        println!("PROVER_NETWORK: Server Recovered requester address: {:?}", hex::encode(&requester));
        let now = chrono::Utc::now().timestamp() as u64;
        let vk_hash = req.body.as_ref().map(|b| b.vk_hash.clone()).unwrap_or_default();
        let programs = self.programs.lock().await;
        let program = programs.get(&vk_hash);
        println!("vk_hash {:?}, program: {:?}", vk_hash, program);
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
                fulfiller: Some(requester.clone()),
                program_uri: program.map(|p| p.program_uri.clone()).unwrap_or_default(),
                program_public_uri: program.map(|p| p.program_uri.clone()).unwrap_or_default(),
                stdin_uri: req.body.as_ref().map(|b| b.stdin_uri.clone()).unwrap_or_default(),
                stdin_public_uri: req.body.as_ref().map(|b| b.stdin_uri.clone()).unwrap_or_default(),
                ..Default::default()
            };
        self.proof_requests.lock().await.insert(request_id, (proof_request, status_response));
        
        Ok(Response::new(response))
    }
    
    async fn get_proof_request_status(
        &self,
        request: Request<GetProofRequestStatusRequest>,
    ) -> Result<Response<GetProofRequestStatusResponse>, Status> {
        let req = request.into_inner();
        println!("PROVER_NETWORK: Server Received status request for ID: {:?}", hex::encode(&req.request_id));
        
        let requests = self.proof_requests.lock().await;
        if let Some((_, status)) = requests.get(&req.request_id) {
            Ok(Response::new(status.clone()))
        } else {
            Err(Status::not_found("Proof request not found"))
        }
    }

    // Implement all other required methods with unimplemented status for now
    async fn fulfill_proof(&self, request: Request<FulfillProofRequest>) -> Result<Response<FulfillProofResponse>, Status> {
        println!("PROVER_NETWORK: fulfill_proof method called");
        let req = request.into_inner();
        let msg_bytes: Vec<u8> = encode_body_for_signing(req.format, req.body.as_ref().ok_or_else(|| Status::invalid_argument("Request body is required"))?)
            .map_err(|e| Status::internal(format!("Failed to encode body for signing: {}", e)))?;
        let requester = match req.body.as_ref() {
            Some(_body) => recover_signer_addr(msg_bytes, &req.signature)
                .map_err(|e| Status::invalid_argument(format!("Failed to recover signer address: {}", e)))?,
            None => return Err(Status::invalid_argument("Request body is required")),
        };
        print!("PROVER_NETWORK: Server fulfill_proof method Recovered requester address: {:?}", hex::encode(&requester));



        let body = req.body.ok_or_else(|| Status::invalid_argument("Request body is required"))?;
        println!("PROVER_NETWORK: domain: {}, request_id: {}, variant: {}, nonce: {}, reserved_metadata: {:?}", hex::encode(&body.domain), hex::encode(&body.request_id), body.variant, body.nonce, body.reserved_metadata);
        let tx_hash_bytes = random::<[u8; 32]>().to_vec();
        let mut requests = self.proof_requests.lock().await;
        if let Some((proof_request, status)) = requests.get_mut(&body.request_id) {
            // Upload proof
            let url = generate_proof_url();
            let client = reqwest::Client::new();
            let upload_response = client
                .put(url.clone())
                .header("Content-Type", "application/binary")
                .body(body.proof.clone())
                .send()
                .await
                .map_err(|e| Status::internal(format!("Failed to upload proof: {}", e)))?;

            if upload_response.status().is_success() {
                println!("✓ Proof uploaded successfully!");
            } else {
                eprintln!("✗ Failed to upload proof. Status: {}", upload_response.status());
                eprintln!("Response: {:?}", upload_response.text().await);
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
            proof_request.fulfiller = Some(requester);
            proof_request.fulfilled_at = Some(now);
            proof_request.execution_status = ExecutionStatus::Executed as i32;
            
            let response = FulfillProofResponse {
                tx_hash: tx_hash_bytes.clone(),
                body: Some(FulfillProofResponseBody {}),
            };
            return Ok(Response::new(response));
        }
        Err(Status::not_found("Proof request not found"))
    }

    async fn execute_proof(&self, _request: Request<ExecuteProofRequest>) -> Result<Response<ExecuteProofResponse>, Status> {
        println!("PROVER_NETWORK: execute_proof method called but not implemented");
        Err(Status::unimplemented("execute_proof not implemented"))
    }

    async fn fail_fulfillment(&self, request: Request<FailFulfillmentRequest>) -> Result<Response<FailFulfillmentResponse>, Status> {
        // TODO validate signature
        // Extract body safely from Option
        let body = request.into_inner().body.ok_or_else(|| Status::invalid_argument("Request body is required"))?;
        
        let mut requests = self.proof_requests.lock().await;
        if let Some((proof_request, status)) = requests.get_mut(&body.request_id) {
            // Update fulfillment status to Unfulfillable
            status.fulfillment_status = FulfillmentStatus::Unfulfillable as i32;
            let now = chrono::Utc::now().timestamp() as u64;
            proof_request.fulfillment_status = status.fulfillment_status;
            proof_request.updated_at = now;
            proof_request.error = body.error.unwrap_or(0); // Unwrap Option<i32> to i32, default to 0
            
            let response = FailFulfillmentResponse {
                tx_hash: proof_request.tx_hash.clone(),
                body: Some(FailFulfillmentResponseBody {}),
            };
            return Ok(Response::new(response));
        }
        Err(Status::not_found("Proof request not found"))
    }

    async fn get_proof_request_details(&self, _request: Request<GetProofRequestDetailsRequest>) -> Result<Response<GetProofRequestDetailsResponse>, Status> {
        println!("PROVER_NETWORK: Server received get_proof_request_details request");
        let req_inner = _request.into_inner();
        println!("PROVER_NETWORK: Request ID received: {:?}", hex::encode(&req_inner.request_id));
        
        let requests = self.proof_requests.lock().await;
        if let Some((request, _)) = requests.get(&req_inner.request_id) {            
            let response = GetProofRequestDetailsResponse {
                request: Some(request.clone()),
            };
            println!("PROVER_NETWORK: Found request, returning details");
            Ok(Response::new(response))
        } else {
            println!("PROVER_NETWORK: Request not found in storage");
            Err(Status::not_found("Proof request not found"))
        }
    }

    async fn get_filtered_proof_requests(&self, _request: Request<GetFilteredProofRequestsRequest>) -> Result<Response<GetFilteredProofRequestsResponse>, Status> {
        // Clone the request data for logging before consuming the request
        let request_data = _request.get_ref().clone();
        let req_inner = _request.into_inner();
        let requests = self.proof_requests.lock().await;
        // println!("PROVER_NETWORK: fulfillment_status: {:?}, fulfiller: {:?}, Total requests in storage: {}", req_inner.fulfillment_status, req_inner.fulfiller.as_ref().map(hex::encode), requests.len());
        println!("TOTAL REQUESTS IN STORAGE: {}", requests.len());
        let mut filtered_requests: Vec<ProofRequest> = requests
            .values()
            .map(|(req, _)| req.clone())
            .filter(|req| {
            // Filter by requester if provided
            if let Some(ref filter_requester) = req_inner.requester {
            if !filter_requester.is_empty() && req.requester != *filter_requester {
                println!("PROVER_NETWORK: Received get_filtered_proof_requests request: {:?}. Filtering by requester. Not matching: {:?}", request_data, req.requester);
                return false;
            }
            }
            
            // Filter by fulfillment status if provided
            if req_inner.fulfillment_status.is_some() && req.fulfillment_status != req_inner.fulfillment_status.unwrap() {
                println!("PROVER_NETWORK: Received get_filtered_proof_requests request: {:?}. Filtering by fulfillment_status. Not matching: {:?}", request_data, req.fulfillment_status);
                return false;
            }
            
            // Filter by execution status if provided
            if req_inner.execution_status.is_some() && req.execution_status != req_inner.execution_status.unwrap() {
                println!("PROVER_NETWORK: Received get_filtered_proof_requests request: {:?}. Filtering by execution_status. Not matching: {:?}", request_data, req.execution_status);
                return false;
            }
            
            // Filter by vk_hash if provided
            if let Some(ref filter_vk_hash) = req_inner.vk_hash {
                if !filter_vk_hash.is_empty() && req.vk_hash != *filter_vk_hash {
                    println!("PROVER_NETWORK: Received get_filtered_proof_requests request: {:?}. Filtering by vk_hash. Not matching: {:?}", request_data, req.vk_hash);
                    return false;
                }
            }
            
            // Filter by version if provided
            if let Some(ref filter_version) = req_inner.version {
                if !filter_version.is_empty() && req.version != *filter_version {
                    println!("PROVER_NETWORK: Received get_filtered_proof_requests request: {:?}. Filtering by version. Not matching: {:?}", request_data, req.version);
                    return false;
                }
            }
            
            // Filter by mode if provided
            if req_inner.mode.is_some() && req.mode != req_inner.mode.unwrap() {
                println!("PROVER_NETWORK: Received get_filtered_proof_requests request: {:?}. Filtering by mode. Not matching: {:?}", request_data, req.mode);
                return false;
            }

            // Filter by minimum_deadline if provided
            if req_inner.minimum_deadline.is_some() && req.deadline <= req_inner.minimum_deadline.unwrap() {
                println!("PROVER_NETWORK: Received get_filtered_proof_requests request: {:?}. Filtering by minimum_deadline. Not matching: received in the request {:?} and stored in the proof_request {:?}", request_data, req_inner.minimum_deadline, req.deadline);
                return false;
            }

            // Filter by fulfiller if provided
            if let Some(ref filter_fulfiller) = req_inner.fulfiller {
                if req.fulfiller.as_ref() != Some(filter_fulfiller) {
                    println!("PROVER_NETWORK: Received get_filtered_proof_requests request: {:?}. Filtering by fulfiller. Not matching: {:?}", request_data, req.fulfiller);
                    return false;
                }
            }

            // Filter by from if provided
            if req_inner.from.is_some() {
                println!("PROVER_NETWORK: Received get_filtered_proof_requests request: {:?}. Filtering by from. Not implemented, ignoring... {:?}", request_data, req_inner.from);
            }

            // Filter by to if provided
            if req_inner.to.is_some() {
                println!("PROVER_NETWORK: Received get_filtered_proof_requests request: {:?}. Filtering by to. Not implemented, ignoring... {:?}", request_data, req_inner.to);
            }

            // Filter by not_bid_by if provided
            if req_inner.not_bid_by.is_some() {
                println!("PROVER_NETWORK: Received get_filtered_proof_requests request: {:?}. Filtering by not_bid_by. Not implemented, ignoring... {:?}", request_data, req_inner.not_bid_by);
            }

            // Filter by execute_fail_cause if provided
            if req_inner.execute_fail_cause.is_some() && req.execute_fail_cause != req_inner.execute_fail_cause.unwrap() {
                println!("PROVER_NETWORK: Received get_filtered_proof_requests request: {:?}. Filtering by execute_fail_cause. Not matching: {:?}", request_data, req.execute_fail_cause);
                return false;
            }

            // Filter by settlement_status if provided
            if req_inner.settlement_status.is_some() && req.settlement_status != req_inner.settlement_status.unwrap() {
                println!("PROVER_NETWORK: Received get_filtered_proof_requests request: {:?}. Filtering by settlement_status. Not matching: {:?}", request_data, req.settlement_status);
                return false;
            }

            // Filter by error if provided
            if req_inner.error.is_some() && req.error != req_inner.error.unwrap() {
                println!("PROVER_NETWORK: Received get_filtered_proof_requests request: {:?}. Filtering by error. Not matching: {:?}", request_data, req.error);
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
        let total_count = filtered_requests.len();
        
        // Apply offset and limit
        let paginated_requests: Vec<ProofRequest> = filtered_requests
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect();
        
        println!("PROVER_NETWORK: Returning {} requests out of {} total", paginated_requests.len(), total_count);
        
        let filtered_requests = paginated_requests;
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
        println!("PROVER_NETWORK: subscribe_proof_requests method called but not implemented");
        Err(Status::unimplemented("subscribe_proof_requests not implemented"))
    }

    async fn get_search_results(&self, _request: Request<GetSearchResultsRequest>) -> Result<Response<GetSearchResultsResponse>, Status> {
        println!("PROVER_NETWORK: get_search_results method called but not implemented");
        Err(Status::unimplemented("get_search_results not implemented"))
    }

    async fn get_proof_request_metrics(&self, _request: Request<GetProofRequestMetricsRequest>) -> Result<Response<GetProofRequestMetricsResponse>, Status> {
        println!("PROVER_NETWORK: get_proof_request_metrics method called but not implemented");
        Err(Status::unimplemented("get_proof_request_metrics not implemented"))
    }

    async fn get_proof_request_graph(&self, _request: Request<GetProofRequestGraphRequest>) -> Result<Response<GetProofRequestGraphResponse>, Status> {
        println!("PROVER_NETWORK: get_proof_request_graph method called but not implemented");
        Err(Status::unimplemented("get_proof_request_graph not implemented"))
    }

    async fn get_analytics_graphs(&self, _request: Request<GetAnalyticsGraphsRequest>) -> Result<Response<GetAnalyticsGraphsResponse>, Status> {
        println!("PROVER_NETWORK: get_analytics_graphs method called but not implemented");
        Err(Status::unimplemented("get_analytics_graphs not implemented"))
    }

    async fn get_overview_graphs(&self, _request: Request<GetOverviewGraphsRequest>) -> Result<Response<GetOverviewGraphsResponse>, Status> {
        println!("PROVER_NETWORK: get_overview_graphs method called but not implemented");
        Err(Status::unimplemented("get_overview_graphs not implemented"))
    }

    async fn get_proof_request_params(&self, _request: Request<GetProofRequestParamsRequest>) -> Result<Response<GetProofRequestParamsResponse>, Status> {
        println!("PROVER_NETWORK: get_proof_request_params method called but not implemented");
        Err(Status::unimplemented("get_proof_request_params not implemented"))
    }

    async fn get_nonce(&self, _request: Request<GetNonceRequest>) -> Result<Response<GetNonceResponse>, Status> {
        Ok(Response::new(GetNonceResponse { nonce: 0 }))
    }

    async fn set_account_name(&self, _request: Request<SetAccountNameRequest>) -> Result<Response<SetAccountNameResponse>, Status> {
        println!("PROVER_NETWORK: set_account_name method called but not implemented");
        Err(Status::unimplemented("set_account_name not implemented"))
    }

    async fn get_account_name(&self, _request: Request<GetAccountNameRequest>) -> Result<Response<GetAccountNameResponse>, Status> {
        println!("PROVER_NETWORK: get_account_name method called but not implemented");
        Err(Status::unimplemented("get_account_name not implemented"))
    }

    async fn get_terms_signature(&self, _request: Request<GetTermsSignatureRequest>) -> Result<Response<GetTermsSignatureResponse>, Status> {
        println!("PROVER_NETWORK: get_terms_signature method called but not implemented");
        Err(Status::unimplemented("get_terms_signature not implemented"))
    }

    async fn set_terms_signature(&self, _request: Request<SetTermsSignatureRequest>) -> Result<Response<SetTermsSignatureResponse>, Status> {
        println!("PROVER_NETWORK: set_terms_signature method called but not implemented");
        Err(Status::unimplemented("set_terms_signature not implemented"))
    }

    async fn get_account(&self, _request: Request<GetAccountRequest>) -> Result<Response<GetAccountResponse>, Status> {
        println!("PROVER_NETWORK: get_account method called but not implemented");
        Err(Status::unimplemented("get_account not implemented"))
    }

    async fn get_owner(&self, _request: Request<GetOwnerRequest>) -> Result<Response<GetOwnerResponse>, Status> {
        // println!("PROVER_NETWORK: Received get_owner request: {:?}", _request.get_ref());
        let acct = _request.into_inner().address;
        Ok(Response::new(GetOwnerResponse { owner: acct.clone() }))
    }

    async fn get_program(&self, _request: Request<GetProgramRequest>) -> Result<Response<GetProgramResponse>, Status> {
        let request_inner = _request.into_inner();
        println!("PROVER_NETWORK: Received get_program request: {:?}", hex::encode(&request_inner.vk_hash));
        // Check if the requested vk_hash exists
        let programs: tokio::sync::MutexGuard<'_, HashMap<Vec<u8>, Program>> = self.programs.lock().await;

        if let Some(program) = programs.get(&request_inner.vk_hash) {            
            let response = GetProgramResponse {
                program: Some(program.clone()),
            };
            Ok(Response::new(response))
        } else {
            Err(Status::not_found("Proof request not found"))
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
        let mut programs: tokio::sync::MutexGuard<'_, HashMap<Vec<u8>, Program>> = self.programs.lock().await;
        programs.insert(vk_hash_key, program.clone());

        let response = CreateProgramResponse {
            tx_hash: random::<[u8; 32]>().to_vec(),
            body: Some(CreateProgramResponseBody {})
        };
        Ok(Response::new(response))
    }

    async fn set_program_name(&self, _request: Request<SetProgramNameRequest>) -> Result<Response<SetProgramNameResponse>, Status> {
        println!("PROVER_NETWORK: set_program_name method called but not implemented");
        Err(Status::unimplemented("set_program_name not implemented"))
    }

    async fn get_balance(&self, _request: Request<GetBalanceRequest>) -> Result<Response<GetBalanceResponse>, Status> {
        println!("PROVER_NETWORK: get_balance method called but not implemented");
        Err(Status::unimplemented("get_balance not implemented"))
    }

    async fn get_filtered_balance_logs(&self, _request: Request<GetFilteredBalanceLogsRequest>) -> Result<Response<GetFilteredBalanceLogsResponse>, Status> {
        println!("PROVER_NETWORK: get_filtered_balance_logs method called but not implemented");
        Err(Status::unimplemented("get_filtered_balance_logs not implemented"))
    }

    async fn add_credit(&self, _request: Request<AddCreditRequest>) -> Result<Response<AddCreditResponse>, Status> {
        println!("PROVER_NETWORK: add_credit method called but not implemented");
        Err(Status::unimplemented("add_credit not implemented"))
    }

    async fn get_latest_bridge_block(&self, _request: Request<GetLatestBridgeBlockRequest>) -> Result<Response<GetLatestBridgeBlockResponse>, Status> {
        println!("PROVER_NETWORK: get_latest_bridge_block method called but not implemented");
        Err(Status::unimplemented("get_latest_bridge_block not implemented"))
    }

    async fn get_gas_price_estimate(&self, _request: Request<GetGasPriceEstimateRequest>) -> Result<Response<GetGasPriceEstimateResponse>, Status> {
        println!("PROVER_NETWORK: get_gas_price_estimate method called but not implemented");
        Err(Status::unimplemented("get_gas_price_estimate not implemented"))
    }

    async fn get_transaction_details(&self, _request: Request<GetTransactionDetailsRequest>) -> Result<Response<GetTransactionDetailsResponse>, Status> {
        println!("PROVER_NETWORK: get_transaction_details method called but not implemented");
        Err(Status::unimplemented("get_transaction_details not implemented"))
    }

    async fn add_reserved_charge(&self, _request: Request<AddReservedChargeRequest>) -> Result<Response<AddReservedChargeResponse>, Status> {
        println!("PROVER_NETWORK: add_reserved_charge method called but not implemented");
        Err(Status::unimplemented("add_reserved_charge not implemented"))
    }

    async fn get_billing_summary(&self, _request: Request<GetBillingSummaryRequest>) -> Result<Response<GetBillingSummaryResponse>, Status> {
        println!("PROVER_NETWORK: get_billing_summary method called but not implemented");
        Err(Status::unimplemented("get_billing_summary not implemented"))
    }

    async fn update_price(&self, _request: Request<UpdatePriceRequest>) -> Result<Response<UpdatePriceResponse>, Status> {
        println!("PROVER_NETWORK: update_price method called but not implemented");
        Err(Status::unimplemented("update_price not implemented"))
    }

    async fn get_filtered_clusters(&self, _request: Request<GetFilteredClustersRequest>) -> Result<Response<GetFilteredClustersResponse>, Status> {
        println!("PROVER_NETWORK: get_filtered_clusters method called but not implemented");
        Err(Status::unimplemented("get_filtered_clusters not implemented"))
    }

    async fn get_usage_summary(&self, _request: Request<GetUsageSummaryRequest>) -> Result<Response<GetUsageSummaryResponse>, Status> {
        println!("PROVER_NETWORK: get_usage_summary method called but not implemented");
        Err(Status::unimplemented("get_usage_summary not implemented"))
    }

    async fn transfer(&self, _request: Request<TransferRequest>) -> Result<Response<TransferResponse>, Status> {
        println!("PROVER_NETWORK: transfer method called but not implemented");
        Err(Status::unimplemented("transfer not implemented"))
    }

    async fn get_withdraw_params(&self, _request: Request<GetWithdrawParamsRequest>) -> Result<Response<GetWithdrawParamsResponse>, Status> {
        println!("PROVER_NETWORK: get_withdraw_params method called but not implemented");
        Err(Status::unimplemented("get_withdraw_params not implemented"))
    }

    async fn withdraw(&self, _request: Request<rpc_types::WithdrawRequest>) -> Result<Response<WithdrawResponse>, Status> {
        println!("PROVER_NETWORK: withdraw method called but not implemented");
        Err(Status::unimplemented("withdraw not implemented"))
    }

    async fn get_filtered_reservations(&self, _request: Request<GetFilteredReservationsRequest>) -> Result<Response<GetFilteredReservationsResponse>, Status> {
        println!("PROVER_NETWORK: get_filtered_reservations method called but not implemented");
        Err(Status::unimplemented("get_filtered_reservations not implemented"))
    }

    async fn add_reservation(&self, _request: Request<AddReservationRequest>) -> Result<Response<AddReservationResponse>, Status> {
        println!("PROVER_NETWORK: add_reservation method called but not implemented");
        Err(Status::unimplemented("add_reservation not implemented"))
    }

    async fn remove_reservation(&self, _request: Request<RemoveReservationRequest>) -> Result<Response<RemoveReservationResponse>, Status> {
        println!("PROVER_NETWORK: remove_reservation method called but not implemented");
        Err(Status::unimplemented("remove_reservation not implemented"))
    }

    async fn bid(&self, _request: Request<BidRequest>) -> Result<Response<BidResponse>, Status> {
        println!("PROVER_NETWORK: bid method called but not implemented");
        Err(Status::unimplemented("bid not implemented"))
    }

    async fn settle(&self, _request: Request<SettleRequest>) -> Result<Response<SettleResponse>, Status> {
        println!("PROVER_NETWORK: settle method called but not implemented");
        Err(Status::unimplemented("settle not implemented"))
    }

    async fn get_provers_by_uptime(&self, _request: Request<GetProversByUptimeRequest>) -> Result<Response<GetProversByUptimeResponse>, Status> {
        println!("PROVER_NETWORK: get_provers_by_uptime method called but not implemented");
        Err(Status::unimplemented("get_provers_by_uptime not implemented"))
    }

    async fn sign_in(&self, _request: Request<SignInRequest>) -> Result<Response<SignInResponse>, Status> {
        println!("PROVER_NETWORK: sign_in method called but not implemented");
        Err(Status::unimplemented("sign_in not implemented"))
    }

    async fn get_onboarded_accounts_count(&self, _request: Request<GetOnboardedAccountsCountRequest>) -> Result<Response<GetOnboardedAccountsCountResponse>, Status> {
        println!("PROVER_NETWORK: get_onboarded_accounts_count method called but not implemented");
        Err(Status::unimplemented("get_onboarded_accounts_count not implemented"))
    }

    async fn get_filtered_onboarded_accounts(&self, _request: Request<GetFilteredOnboardedAccountsRequest>) -> Result<Response<GetFilteredOnboardedAccountsResponse>, Status> {
        println!("PROVER_NETWORK: get_filtered_onboarded_accounts method called but not implemented");
        Err(Status::unimplemented("get_filtered_onboarded_accounts not implemented"))
    }

    async fn get_leaderboard(&self, _request: Request<GetLeaderboardRequest>) -> Result<Response<GetLeaderboardResponse>, Status> {
        println!("PROVER_NETWORK: get_leaderboard method called but not implemented");
        Err(Status::unimplemented("get_leaderboard not implemented"))
    }

    async fn get_leaderboard_stats(&self, _request: Request<GetLeaderboardStatsRequest>) -> Result<Response<GetLeaderboardStatsResponse>, Status> {
        println!("PROVER_NETWORK: get_leaderboard_stats method called but not implemented");
        Err(Status::unimplemented("get_leaderboard_stats not implemented"))
    }

    async fn get_codes(&self, _request: Request<GetCodesRequest>) -> Result<Response<GetCodesResponse>, Status> {
        println!("PROVER_NETWORK: get_codes method called but not implemented");
        Err(Status::unimplemented("get_codes not implemented"))
    }

    async fn redeem_code(&self, _request: Request<RedeemCodeRequest>) -> Result<Response<RedeemCodeResponse>, Status> {
        println!("PROVER_NETWORK: redeem_code method called but not implemented");
        Err(Status::unimplemented("redeem_code not implemented"))
    }

    async fn connect_twitter(&self, _request: Request<ConnectTwitterRequest>) -> Result<Response<ConnectTwitterResponse>, Status> {
        println!("PROVER_NETWORK: connect_twitter method called but not implemented");
        Err(Status::unimplemented("connect_twitter not implemented"))
    }

    async fn complete_onboarding(&self, _request: Request<CompleteOnboardingRequest>) -> Result<Response<CompleteOnboardingResponse>, Status> {
        println!("PROVER_NETWORK: complete_onboarding method called but not implemented");
        Err(Status::unimplemented("complete_onboarding not implemented"))
    }

    async fn set_use_twitter_handle(&self, _request: Request<SetUseTwitterHandleRequest>) -> Result<Response<SetUseTwitterHandleResponse>, Status> {
        println!("PROVER_NETWORK: set_use_twitter_handle method called but not implemented");
        Err(Status::unimplemented("set_use_twitter_handle not implemented"))
    }

    async fn set_use_twitter_image(&self, _request: Request<SetUseTwitterImageRequest>) -> Result<Response<SetUseTwitterImageResponse>, Status> {
        println!("PROVER_NETWORK: set_use_twitter_image method called but not implemented");
        Err(Status::unimplemented("set_use_twitter_image not implemented"))
    }

    async fn request_random_proof(&self, _request: Request<RequestRandomProofRequest>) -> Result<Response<RequestRandomProofResponse>, Status> {
        println!("PROVER_NETWORK: request_random_proof method called but not implemented");
        Err(Status::unimplemented("request_random_proof not implemented"))
    }

    async fn submit_captcha_game(&self, _request: Request<SubmitCaptchaGameRequest>) -> Result<Response<SubmitCaptchaGameResponse>, Status> {
        println!("PROVER_NETWORK: submit_captcha_game method called but not implemented");
        Err(Status::unimplemented("submit_captcha_game not implemented"))
    }

    async fn redeem_stars(&self, _request: Request<RedeemStarsRequest>) -> Result<Response<RedeemStarsResponse>, Status> {
        println!("PROVER_NETWORK: redeem_stars method called but not implemented");
        Err(Status::unimplemented("redeem_stars not implemented"))
    }

    async fn get_flappy_leaderboard(&self, _request: Request<GetFlappyLeaderboardRequest>) -> Result<Response<GetFlappyLeaderboardResponse>, Status> {
        println!("PROVER_NETWORK: get_flappy_leaderboard method called but not implemented");
        Err(Status::unimplemented("get_flappy_leaderboard not implemented"))
    }

    async fn set_turbo_high_score(&self, _request: Request<SetTurboHighScoreRequest>) -> Result<Response<SetTurboHighScoreResponse>, Status> {
        println!("PROVER_NETWORK: set_turbo_high_score method called but not implemented");
        Err(Status::unimplemented("set_turbo_high_score not implemented"))
    }

    async fn submit_quiz_game(&self, _request: Request<SubmitQuizGameRequest>) -> Result<Response<SubmitQuizGameResponse>, Status> {
        println!("PROVER_NETWORK: submit_quiz_game method called but not implemented");
        Err(Status::unimplemented("submit_quiz_game not implemented"))
    }

    async fn get_turbo_leaderboard(&self, _request: Request<GetTurboLeaderboardRequest>) -> Result<Response<GetTurboLeaderboardResponse>, Status> {
        println!("PROVER_NETWORK: get_turbo_leaderboard method called but not implemented");
        Err(Status::unimplemented("get_turbo_leaderboard not implemented"))
    }

    async fn submit_eth_block_metadata(&self, _request: Request<SubmitEthBlockMetadataRequest>) -> Result<Response<SubmitEthBlockMetadataResponse>, Status> {
        println!("PROVER_NETWORK: submit_eth_block_metadata method called but not implemented");
        Err(Status::unimplemented("submit_eth_block_metadata not implemented"))
    }

    async fn get_filtered_eth_block_requests(&self, _request: Request<GetFilteredEthBlockRequestsRequest>) -> Result<Response<GetFilteredEthBlockRequestsResponse>, Status> {
        println!("PROVER_NETWORK: get_filtered_eth_block_requests method called but not implemented");
        Err(Status::unimplemented("get_filtered_eth_block_requests not implemented"))
    }

    async fn set2048_high_score(&self, _request: Request<Set2048HighScoreRequest>) -> Result<Response<Set2048HighScoreResponse>, Status> {
        println!("PROVER_NETWORK: set2048_high_score method called but not implemented");
        Err(Status::unimplemented("set2048_high_score not implemented"))
    }

    async fn set_volleyball_high_score(&self, _request: Request<SetVolleyballHighScoreRequest>) -> Result<Response<SetVolleyballHighScoreResponse>, Status> {
        println!("PROVER_NETWORK: set_volleyball_high_score method called but not implemented");
        Err(Status::unimplemented("set_volleyball_high_score not implemented"))
    }

    async fn get_eth_block_request_metrics(&self, _request: Request<GetEthBlockRequestMetricsRequest>) -> Result<Response<GetEthBlockRequestMetricsResponse>, Status> {
        println!("PROVER_NETWORK: get_eth_block_request_metrics method called but not implemented");
        Err(Status::unimplemented("get_eth_block_request_metrics not implemented"))
    }

    async fn set_turbo_time_trial_high_score(&self, _request: Request<SetTurboTimeTrialHighScoreRequest>) -> Result<Response<SetTurboTimeTrialHighScoreResponse>, Status> {
        println!("PROVER_NETWORK: set_turbo_time_trial_high_score method called but not implemented");
        Err(Status::unimplemented("set_turbo_time_trial_high_score not implemented"))
    }

    async fn set_coin_craze_high_score(&self, _request: Request<SetCoinCrazeHighScoreRequest>) -> Result<Response<SetCoinCrazeHighScoreResponse>, Status> {
        println!("PROVER_NETWORK: set_coin_craze_high_score method called but not implemented");
        Err(Status::unimplemented("set_coin_craze_high_score not implemented"))
    }

    async fn set_lean_high_score(&self, _request: Request<SetLeanHighScoreRequest>) -> Result<Response<SetLeanHighScoreResponse>, Status> {
        println!("PROVER_NETWORK: set_lean_high_score method called but not implemented");
        Err(Status::unimplemented("set_lean_high_score not implemented"))
    }

    async fn set_flow_high_score(&self, _request: Request<SetFlowHighScoreRequest>) -> Result<Response<SetFlowHighScoreResponse>, Status> {
        println!("PROVER_NETWORK: set_flow_high_score method called but not implemented");
        Err(Status::unimplemented("set_flow_high_score not implemented"))
    }

    async fn set_rollup_high_score(&self, _request: Request<SetRollupHighScoreRequest>) -> Result<Response<SetRollupHighScoreResponse>, Status> {
        println!("PROVER_NETWORK: set_rollup_high_score method called but not implemented");
        Err(Status::unimplemented("set_rollup_high_score not implemented"))
    }

    async fn get_pending_stars(&self, _request: Request<GetPendingStarsRequest>) -> Result<Response<GetPendingStarsResponse>, Status> {
        println!("PROVER_NETWORK: get_pending_stars method called but not implemented");
        Err(Status::unimplemented("get_pending_stars not implemented"))
    }

    async fn get_whitelist_status(&self, _request: Request<GetWhitelistStatusRequest>) -> Result<Response<GetWhitelistStatusResponse>, Status> {
        println!("PROVER_NETWORK: get_whitelist_status method called but not implemented");
        Err(Status::unimplemented("get_whitelist_status not implemented"))
    }

    async fn claim_gpu(&self, _request: Request<ClaimGpuRequest>) -> Result<Response<ClaimGpuResponse>, Status> {
        println!("PROVER_NETWORK: claim_gpu method called but not implemented");
        Err(Status::unimplemented("claim_gpu not implemented"))
    }

    async fn set_gpu_variant(&self, _request: Request<SetGpuVariantRequest>) -> Result<Response<SetGpuVariantResponse>, Status> {
        println!("PROVER_NETWORK: set_gpu_variant method called but not implemented");
        Err(Status::unimplemented("set_gpu_variant not implemented"))
    }

    async fn link_whitelisted_twitter(&self, _request: Request<LinkWhitelistedTwitterRequest>) -> Result<Response<LinkWhitelistedTwitterResponse>, Status> {
        println!("PROVER_NETWORK: link_whitelisted_twitter method called but not implemented");
        Err(Status::unimplemented("link_whitelisted_twitter not implemented"))
    }

    async fn retrieve_proving_key(&self, _request: Request<RetrieveProvingKeyRequest>) -> Result<Response<RetrieveProvingKeyResponse>, Status> {
        println!("PROVER_NETWORK: retrieve_proving_key method called but not implemented");
        Err(Status::unimplemented("retrieve_proving_key not implemented"))
    }

    async fn link_whitelisted_github(&self, _request: Request<LinkWhitelistedGithubRequest>) -> Result<Response<LinkWhitelistedGithubResponse>, Status> {
        println!("PROVER_NETWORK: link_whitelisted_github method called but not implemented");
        Err(Status::unimplemented("link_whitelisted_github not implemented"))
    }

    async fn link_whitelisted_discord(&self, _request: Request<LinkWhitelistedDiscordRequest>) -> Result<Response<LinkWhitelistedDiscordResponse>, Status> {
        println!("PROVER_NETWORK: link_whitelisted_discord method called but not implemented");
        Err(Status::unimplemented("link_whitelisted_discord not implemented"))
    }

    async fn get_prover_leaderboard(&self, _request: Request<GetProverLeaderboardRequest>) -> Result<Response<GetProverLeaderboardResponse>, Status> {
        println!("PROVER_NETWORK: get_prover_leaderboard method called but not implemented");
        Err(Status::unimplemented("get_prover_leaderboard not implemented"))
    }

    async fn get_filtered_gpus(&self, _request: Request<GetFilteredGpusRequest>) -> Result<Response<GetFilteredGpusResponse>, Status> {
        println!("PROVER_NETWORK: get_filtered_gpus method called but not implemented");
        Err(Status::unimplemented("get_filtered_gpus not implemented"))
    }

    async fn set_gpu_coordinates(&self, _request: Request<SetGpuCoordinatesRequest>) -> Result<Response<SetGpuCoordinatesResponse>, Status> {
        println!("PROVER_NETWORK: set_gpu_coordinates method called but not implemented");
        Err(Status::unimplemented("set_gpu_coordinates not implemented"))
    }

    async fn get_points(&self, _request: Request<GetPointsRequest>) -> Result<Response<GetPointsResponse>, Status> {
        println!("PROVER_NETWORK: get_points method called but not implemented");
        Err(Status::unimplemented("get_points not implemented"))
    }

    async fn process_clicks(&self, _request: Request<ProcessClicksRequest>) -> Result<Response<ProcessClicksResponse>, Status> {
        println!("PROVER_NETWORK: process_clicks method called but not implemented");
        Err(Status::unimplemented("process_clicks not implemented"))
    }

    async fn purchase_upgrade(&self, _request: Request<PurchaseUpgradeRequest>) -> Result<Response<PurchaseUpgradeResponse>, Status> {
        println!("PROVER_NETWORK: purchase_upgrade method called but not implemented");
        Err(Status::unimplemented("purchase_upgrade not implemented"))
    }

    async fn bet(&self, _request: Request<BetRequest>) -> Result<Response<BetResponse>, Status> {
        println!("PROVER_NETWORK: bet method called but not implemented");
        Err(Status::unimplemented("bet not implemented"))
    }

    async fn get_contest_details(&self, _request: Request<GetContestDetailsRequest>) -> Result<Response<GetContestDetailsResponse>, Status> {
        println!("PROVER_NETWORK: get_contest_details method called but not implemented");
        Err(Status::unimplemented("get_contest_details not implemented"))
    }

    async fn get_latest_contest(&self, _request: Request<GetLatestContestRequest>) -> Result<Response<GetLatestContestResponse>, Status> {
        println!("PROVER_NETWORK: get_latest_contest method called but not implemented");
        Err(Status::unimplemented("get_latest_contest not implemented"))
    }

    async fn get_contest_bettors(&self, _request: Request<GetContestBettorsRequest>) -> Result<Response<GetContestBettorsResponse>, Status> {
        println!("PROVER_NETWORK: get_contest_bettors method called but not implemented");
        Err(Status::unimplemented("get_contest_bettors not implemented"))
    }

    async fn get_gpu_metrics(&self, _request: Request<GetGpuMetricsRequest>) -> Result<Response<GetGpuMetricsResponse>, Status> {
        println!("PROVER_NETWORK: get_gpu_metrics method called but not implemented");
        Err(Status::unimplemented("get_gpu_metrics not implemented"))
    }

    async fn get_filtered_prover_activity(&self, _request: Request<GetFilteredProverActivityRequest>) -> Result<Response<GetFilteredProverActivityResponse>, Status> {
        println!("PROVER_NETWORK: get_filtered_prover_activity method called but not implemented");
        Err(Status::unimplemented("get_filtered_prover_activity not implemented"))
    }

    async fn get_prover_metrics(&self, _request: Request<GetProverMetricsRequest>) -> Result<Response<GetProverMetricsResponse>, Status> {
        println!("PROVER_NETWORK: get_prover_metrics method called but not implemented");
        Err(Status::unimplemented("get_prover_metrics not implemented"))
    }

    async fn get_filtered_bet_history(&self, _request: Request<GetFilteredBetHistoryRequest>) -> Result<Response<GetFilteredBetHistoryResponse>, Status> {
        println!("PROVER_NETWORK: get_filtered_bet_history method called but not implemented");
        Err(Status::unimplemented("get_filtered_bet_history not implemented"))
    }

    async fn get_gpu_team_stats(&self, _request: Request<GetGpuTeamStatsRequest>) -> Result<Response<GetGpuTeamStatsResponse>, Status> {
        println!("PROVER_NETWORK: get_gpu_team_stats method called but not implemented");
        Err(Status::unimplemented("get_gpu_team_stats not implemented"))
    }

    async fn get_config_values(&self, _request: Request<GetConfigValuesRequest>) -> Result<Response<GetConfigValuesResponse>, Status> {
        println!("PROVER_NETWORK: get_config_values method called but not implemented");
        Err(Status::unimplemented("get_config_values not implemented"))
    }

    async fn get_prover_stats(&self, _request: Request<GetProverStatsRequest>) -> Result<Response<GetProverStatsResponse>, Status> {
        println!("PROVER_NETWORK: get_prover_stats method called but not implemented");
        Err(Status::unimplemented("get_prover_stats not implemented"))
    }

    async fn get_filtered_prover_stats(&self, _request: Request<GetFilteredProverStatsRequest>) -> Result<Response<GetFilteredProverStatsResponse>, Status> {
        println!("PROVER_NETWORK: get_filtered_prover_stats method called but not implemented");
        Err(Status::unimplemented("get_filtered_prover_stats not implemented"))
    }

    async fn get_prover_stats_detail(&self, _request: Request<GetProverStatsDetailRequest>) -> Result<Response<GetProverStatsDetailResponse>, Status> {
        println!("PROVER_NETWORK: get_prover_stats_detail method called but not implemented");
        Err(Status::unimplemented("get_prover_stats_detail not implemented"))
    }

    async fn get_prover_search_results(&self, _request: Request<GetProverSearchResultsRequest>) -> Result<Response<GetProverSearchResultsResponse>, Status> {
        println!("PROVER_NETWORK: get_prover_search_results method called but not implemented");
        Err(Status::unimplemented("get_prover_search_results not implemented"))
    }

    async fn get_filtered_bid_history(&self, _request: Request<GetFilteredBidHistoryRequest>) -> Result<Response<GetFilteredBidHistoryResponse>, Status> {
        println!("PROVER_NETWORK: get_filtered_bid_history method called but not implemented");
        Err(Status::unimplemented("get_filtered_bid_history not implemented"))
    }

    async fn get_tee_whitelist_status(&self, _request: Request<GetTeeWhitelistStatusRequest>) -> Result<Response<GetTeeWhitelistStatusResponse>, Status> {
        println!("PROVER_NETWORK: get_tee_whitelist_status method called but not implemented");
        Err(Status::unimplemented("get_tee_whitelist_status not implemented"))
    }

    async fn get_settlement_request(&self, _request: Request<GetSettlementRequestRequest>) -> Result<Response<GetSettlementRequestResponse>, Status> {
        println!("PROVER_NETWORK: get_settlement_request method called but not implemented");
        Err(Status::unimplemented("get_settlement_request not implemented"))
    }

    async fn get_filtered_settlement_requests(&self, _request: Request<GetFilteredSettlementRequestsRequest>) -> Result<Response<GetFilteredSettlementRequestsResponse>, Status> {
        println!("PROVER_NETWORK: get_filtered_settlement_requests method called but not implemented");
        Err(Status::unimplemented("get_filtered_settlement_requests not implemented"))
    }

    async fn get_filtered_provers(&self, _request: Request<GetFilteredProversRequest>) -> Result<Response<GetFilteredProversResponse>, Status> {
        println!("PROVER_NETWORK: get_filtered_provers method called but not implemented");
        Err(Status::unimplemented("get_filtered_provers not implemented"))
    }

    async fn get_prover_stake_balance(&self, _request: Request<GetProverStakeBalanceRequest>) -> Result<Response<GetProverStakeBalanceResponse>, Status> {
        println!("PROVER_NETWORK: get_prover_stake_balance method called but not implemented");
        Err(Status::unimplemented("get_prover_stake_balance not implemented"))
    }

    async fn get_filtered_staker_stake_balance_logs(&self, _request: Request<GetFilteredStakerStakeBalanceLogsRequest>) -> Result<Response<GetFilteredStakerStakeBalanceLogsResponse>, Status> {
        println!("PROVER_NETWORK: get_filtered_staker_stake_balance_logs method called but not implemented");
        Err(Status::unimplemented("get_filtered_staker_stake_balance_logs not implemented"))
    }

    async fn get_filtered_prover_stake_balance_logs(&self, _request: Request<GetFilteredProverStakeBalanceLogsRequest>) -> Result<Response<GetFilteredProverStakeBalanceLogsResponse>, Status> {
        println!("PROVER_NETWORK: get_filtered_prover_stake_balance_logs method called but not implemented");
        Err(Status::unimplemented("get_filtered_prover_stake_balance_logs not implemented"))
    }

    async fn get_delegation_params(&self, _request: Request<GetDelegationParamsRequest>) -> Result<Response<GetDelegationParamsResponse>, Status> {
        println!("PROVER_NETWORK: get_delegation_params method called but not implemented");
        Err(Status::unimplemented("get_delegation_params not implemented"))
    }

    async fn set_delegation(&self, _request: Request<SetDelegationRequest>) -> Result<Response<SetDelegationResponse>, Status> {
        println!("PROVER_NETWORK: set_delegation method called but not implemented");
        Err(Status::unimplemented("set_delegation not implemented"))
    }

    async fn get_delegation(&self, _request: Request<GetDelegationRequest>) -> Result<Response<GetDelegationResponse>, Status> {
        println!("PROVER_NETWORK: get_delegation method called but not implemented");
        Err(Status::unimplemented("get_delegation not implemented"))
    }

    async fn get_filtered_withdrawal_receipts(&self, _request: Request<GetFilteredWithdrawalReceiptsRequest>) -> Result<Response<GetFilteredWithdrawalReceiptsResponse>, Status> {
        println!("PROVER_NETWORK: get_filtered_withdrawal_receipts method called but not implemented");
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

fn generate_proof_url() -> String {
    // Generate a URL pointing to our HTTP server
    // The client will use this URL to PUT the artifact data
    format!("http://localhost:8082/artifacts/Proof/{}", hex::encode(random::<[u8; 16]>()))
}