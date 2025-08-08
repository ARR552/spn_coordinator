use anyhow::Result;
use rpc_types::*;
use std::collections::HashMap;
use tokio::sync::{Mutex, mpsc};
use tonic::{transport::Server, Request, Response, Status};
use tonic_reflection::server::{Builder as ReflBuilder};
use uuid::Uuid;
use rand::random;

/// Real gRPC service implementation for ProverNetwork
#[derive(Debug, Default)]
pub struct ProverNetworkServiceImpl {
    /// TODO Store proof requests in memory (in real implementation this would be a database)  
    requests: Mutex<HashMap<Vec<u8>, (ProofRequest, GetProofRequestStatusResponse)>>,
}

#[tonic::async_trait]
impl prover_network_server::ProverNetwork for ProverNetworkServiceImpl {
    async fn request_proof(
        &self,
        request: Request<RequestProofRequest>,
    ) -> Result<Response<RequestProofResponse>, Status> {
        let req = request.into_inner();
        println!("Received proof request: {:?}", req);
        
        // Generate a unique request ID
        let request_id = Uuid::new_v4().as_bytes().to_vec();
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
        let now = chrono::Utc::now().timestamp() as u64;
        let proof_request = ProofRequest {
                request_id: request_id.clone(),
                vk_hash: req.body.as_ref().map(|b| b.vk_hash.clone()).unwrap_or_default(),
                version: req.body.as_ref().map(|b| b.version.clone()).unwrap_or_default(),
                mode:    req.body.as_ref().map(|b| b.mode.clone()).unwrap_or_default(),
                strategy: req.body.as_ref().map(|b| b.strategy.clone()).unwrap_or_default(),
                stdin_uri: req.body.as_ref().map(|b| b.stdin_uri.clone()).unwrap_or_default(),
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
                ..Default::default()
            };
        self.requests.lock().await.insert(request_id, (proof_request, status_response));
        
        Ok(Response::new(response))
    }
    
    async fn get_proof_request_status(
        &self,
        request: Request<GetProofRequestStatusRequest>,
    ) -> Result<Response<GetProofRequestStatusResponse>, Status> {
        let req = request.into_inner();
        println!("Received status request for ID: {:?}", hex::encode(&req.request_id));
        
        let requests = self.requests.lock().await;
        if let Some((_, status)) = requests.get(&req.request_id) {
            Ok(Response::new(status.clone()))
        } else {
            Err(Status::not_found("Proof request not found"))
        }
    }

    // Implement all other required methods with unimplemented status for now
    async fn fulfill_proof(&self, _request: Request<FulfillProofRequest>) -> Result<Response<FulfillProofResponse>, Status> {
        Err(Status::unimplemented("fulfill_proof not implemented"))
    }

    async fn execute_proof(&self, _request: Request<ExecuteProofRequest>) -> Result<Response<ExecuteProofResponse>, Status> {
        Err(Status::unimplemented("execute_proof not implemented"))
    }

    async fn fail_fulfillment(&self, _request: Request<FailFulfillmentRequest>) -> Result<Response<FailFulfillmentResponse>, Status> {
        Err(Status::unimplemented("fail_fulfillment not implemented"))
    }

    async fn get_proof_request_details(&self, _request: Request<GetProofRequestDetailsRequest>) -> Result<Response<GetProofRequestDetailsResponse>, Status> {
        let requests = self.requests.lock().await;
        if let Some((request, _)) = requests.get(&_request.into_inner().request_id) {            
            let response = GetProofRequestDetailsResponse {
                request: Some(request.clone()),
            };
            Ok(Response::new(response))
        } else {
            Err(Status::not_found("Proof request not found"))
        }
    }

    async fn get_filtered_proof_requests(&self, _request: Request<GetFilteredProofRequestsRequest>) -> Result<Response<GetFilteredProofRequestsResponse>, Status> {
        println!("Received get_filtered_proof_requests request: {:?}", _request.get_ref());
        // TODO implemente the filtering logic
        let requests = self.requests.lock().await;
        let all_requests: Vec<ProofRequest> = requests.values().map(|(req, _)| req.clone()).collect();
        Ok(Response::new(GetFilteredProofRequestsResponse {
            requests: all_requests,
        }))
    }

    type SubscribeProofRequestsStream = std::pin::Pin<Box<dyn tokio_stream::Stream<Item = Result<ProofRequest, Status>> + Send>>;

    async fn subscribe_proof_requests(&self, _request: Request<GetFilteredProofRequestsRequest>) -> Result<Response<Self::SubscribeProofRequestsStream>, Status> {
        Err(Status::unimplemented("subscribe_proof_requests not implemented"))
    }

    async fn get_search_results(&self, _request: Request<GetSearchResultsRequest>) -> Result<Response<GetSearchResultsResponse>, Status> {
        Err(Status::unimplemented("get_search_results not implemented"))
    }

    async fn get_proof_request_metrics(&self, _request: Request<GetProofRequestMetricsRequest>) -> Result<Response<GetProofRequestMetricsResponse>, Status> {
        Err(Status::unimplemented("get_proof_request_metrics not implemented"))
    }

    async fn get_proof_request_graph(&self, _request: Request<GetProofRequestGraphRequest>) -> Result<Response<GetProofRequestGraphResponse>, Status> {
        Err(Status::unimplemented("get_proof_request_graph not implemented"))
    }

    async fn get_analytics_graphs(&self, _request: Request<GetAnalyticsGraphsRequest>) -> Result<Response<GetAnalyticsGraphsResponse>, Status> {
        Err(Status::unimplemented("get_analytics_graphs not implemented"))
    }

    async fn get_overview_graphs(&self, _request: Request<GetOverviewGraphsRequest>) -> Result<Response<GetOverviewGraphsResponse>, Status> {
        Err(Status::unimplemented("get_overview_graphs not implemented"))
    }

    async fn get_proof_request_params(&self, _request: Request<GetProofRequestParamsRequest>) -> Result<Response<GetProofRequestParamsResponse>, Status> {
        Err(Status::unimplemented("get_proof_request_params not implemented"))
    }

    async fn get_nonce(&self, _request: Request<GetNonceRequest>) -> Result<Response<GetNonceResponse>, Status> {
        Err(Status::unimplemented("get_nonce not implemented"))
    }

    async fn set_account_name(&self, _request: Request<SetAccountNameRequest>) -> Result<Response<SetAccountNameResponse>, Status> {
        Err(Status::unimplemented("set_account_name not implemented"))
    }

    async fn get_account_name(&self, _request: Request<GetAccountNameRequest>) -> Result<Response<GetAccountNameResponse>, Status> {
        Err(Status::unimplemented("get_account_name not implemented"))
    }

    async fn get_terms_signature(&self, _request: Request<GetTermsSignatureRequest>) -> Result<Response<GetTermsSignatureResponse>, Status> {
        Err(Status::unimplemented("get_terms_signature not implemented"))
    }

    async fn set_terms_signature(&self, _request: Request<SetTermsSignatureRequest>) -> Result<Response<SetTermsSignatureResponse>, Status> {
        Err(Status::unimplemented("set_terms_signature not implemented"))
    }

    async fn get_account(&self, _request: Request<GetAccountRequest>) -> Result<Response<GetAccountResponse>, Status> {
        Err(Status::unimplemented("get_account not implemented"))
    }

    async fn get_owner(&self, _request: Request<GetOwnerRequest>) -> Result<Response<GetOwnerResponse>, Status> {
        // println!("Received get_owner request: {:?}", _request.get_ref());
        let acct = _request.into_inner().address.to_ascii_lowercase();
        Ok(Response::new(GetOwnerResponse { owner: acct.clone() }))
    }

    async fn get_program(&self, _request: Request<GetProgramRequest>) -> Result<Response<GetProgramResponse>, Status> {
        Err(Status::unimplemented("get_program not implemented"))
    }

    async fn create_program(&self, _request: Request<CreateProgramRequest>) -> Result<Response<CreateProgramResponse>, Status> {
        Err(Status::unimplemented("create_program not implemented"))
    }

    async fn set_program_name(&self, _request: Request<SetProgramNameRequest>) -> Result<Response<SetProgramNameResponse>, Status> {
        Err(Status::unimplemented("set_program_name not implemented"))
    }

    async fn get_balance(&self, _request: Request<GetBalanceRequest>) -> Result<Response<GetBalanceResponse>, Status> {
        Err(Status::unimplemented("get_balance not implemented"))
    }

    async fn get_filtered_balance_logs(&self, _request: Request<GetFilteredBalanceLogsRequest>) -> Result<Response<GetFilteredBalanceLogsResponse>, Status> {
        Err(Status::unimplemented("get_filtered_balance_logs not implemented"))
    }

    async fn add_credit(&self, _request: Request<AddCreditRequest>) -> Result<Response<AddCreditResponse>, Status> {
        Err(Status::unimplemented("add_credit not implemented"))
    }

    async fn get_latest_bridge_block(&self, _request: Request<GetLatestBridgeBlockRequest>) -> Result<Response<GetLatestBridgeBlockResponse>, Status> {
        Err(Status::unimplemented("get_latest_bridge_block not implemented"))
    }

    async fn get_gas_price_estimate(&self, _request: Request<GetGasPriceEstimateRequest>) -> Result<Response<GetGasPriceEstimateResponse>, Status> {
        Err(Status::unimplemented("get_gas_price_estimate not implemented"))
    }

    async fn get_transaction_details(&self, _request: Request<GetTransactionDetailsRequest>) -> Result<Response<GetTransactionDetailsResponse>, Status> {
        Err(Status::unimplemented("get_transaction_details not implemented"))
    }

    async fn add_reserved_charge(&self, _request: Request<AddReservedChargeRequest>) -> Result<Response<AddReservedChargeResponse>, Status> {
        Err(Status::unimplemented("add_reserved_charge not implemented"))
    }

    async fn get_billing_summary(&self, _request: Request<GetBillingSummaryRequest>) -> Result<Response<GetBillingSummaryResponse>, Status> {
        Err(Status::unimplemented("get_billing_summary not implemented"))
    }

    async fn update_price(&self, _request: Request<UpdatePriceRequest>) -> Result<Response<UpdatePriceResponse>, Status> {
        Err(Status::unimplemented("update_price not implemented"))
    }

    async fn get_filtered_clusters(&self, _request: Request<GetFilteredClustersRequest>) -> Result<Response<GetFilteredClustersResponse>, Status> {
        Err(Status::unimplemented("get_filtered_clusters not implemented"))
    }

    async fn get_usage_summary(&self, _request: Request<GetUsageSummaryRequest>) -> Result<Response<GetUsageSummaryResponse>, Status> {
        Err(Status::unimplemented("get_usage_summary not implemented"))
    }

    async fn transfer(&self, _request: Request<TransferRequest>) -> Result<Response<TransferResponse>, Status> {
        Err(Status::unimplemented("transfer not implemented"))
    }

    async fn get_withdraw_params(&self, _request: Request<GetWithdrawParamsRequest>) -> Result<Response<GetWithdrawParamsResponse>, Status> {
        Err(Status::unimplemented("get_withdraw_params not implemented"))
    }

    async fn withdraw(&self, _request: Request<rpc_types::WithdrawRequest>) -> Result<Response<WithdrawResponse>, Status> {
        Err(Status::unimplemented("withdraw not implemented"))
    }

    async fn get_filtered_reservations(&self, _request: Request<GetFilteredReservationsRequest>) -> Result<Response<GetFilteredReservationsResponse>, Status> {
        Err(Status::unimplemented("get_filtered_reservations not implemented"))
    }

    async fn add_reservation(&self, _request: Request<AddReservationRequest>) -> Result<Response<AddReservationResponse>, Status> {
        Err(Status::unimplemented("add_reservation not implemented"))
    }

    async fn remove_reservation(&self, _request: Request<RemoveReservationRequest>) -> Result<Response<RemoveReservationResponse>, Status> {
        Err(Status::unimplemented("remove_reservation not implemented"))
    }

    async fn bid(&self, _request: Request<BidRequest>) -> Result<Response<BidResponse>, Status> {
        Err(Status::unimplemented("bid not implemented"))
    }

    async fn settle(&self, _request: Request<SettleRequest>) -> Result<Response<SettleResponse>, Status> {
        Err(Status::unimplemented("settle not implemented"))
    }

    async fn get_provers_by_uptime(&self, _request: Request<GetProversByUptimeRequest>) -> Result<Response<GetProversByUptimeResponse>, Status> {
        Err(Status::unimplemented("get_provers_by_uptime not implemented"))
    }

    async fn sign_in(&self, _request: Request<SignInRequest>) -> Result<Response<SignInResponse>, Status> {
        Err(Status::unimplemented("sign_in not implemented"))
    }

    async fn get_onboarded_accounts_count(&self, _request: Request<GetOnboardedAccountsCountRequest>) -> Result<Response<GetOnboardedAccountsCountResponse>, Status> {
        Err(Status::unimplemented("get_onboarded_accounts_count not implemented"))
    }

    async fn get_filtered_onboarded_accounts(&self, _request: Request<GetFilteredOnboardedAccountsRequest>) -> Result<Response<GetFilteredOnboardedAccountsResponse>, Status> {
        Err(Status::unimplemented("get_filtered_onboarded_accounts not implemented"))
    }

    async fn get_leaderboard(&self, _request: Request<GetLeaderboardRequest>) -> Result<Response<GetLeaderboardResponse>, Status> {
        Err(Status::unimplemented("get_leaderboard not implemented"))
    }

    async fn get_leaderboard_stats(&self, _request: Request<GetLeaderboardStatsRequest>) -> Result<Response<GetLeaderboardStatsResponse>, Status> {
        Err(Status::unimplemented("get_leaderboard_stats not implemented"))
    }

    async fn get_codes(&self, _request: Request<GetCodesRequest>) -> Result<Response<GetCodesResponse>, Status> {
        Err(Status::unimplemented("get_codes not implemented"))
    }

    async fn redeem_code(&self, _request: Request<RedeemCodeRequest>) -> Result<Response<RedeemCodeResponse>, Status> {
        Err(Status::unimplemented("redeem_code not implemented"))
    }

    async fn connect_twitter(&self, _request: Request<ConnectTwitterRequest>) -> Result<Response<ConnectTwitterResponse>, Status> {
        Err(Status::unimplemented("connect_twitter not implemented"))
    }

    async fn complete_onboarding(&self, _request: Request<CompleteOnboardingRequest>) -> Result<Response<CompleteOnboardingResponse>, Status> {
        Err(Status::unimplemented("complete_onboarding not implemented"))
    }

    async fn set_use_twitter_handle(&self, _request: Request<SetUseTwitterHandleRequest>) -> Result<Response<SetUseTwitterHandleResponse>, Status> {
        Err(Status::unimplemented("set_use_twitter_handle not implemented"))
    }

    async fn set_use_twitter_image(&self, _request: Request<SetUseTwitterImageRequest>) -> Result<Response<SetUseTwitterImageResponse>, Status> {
        Err(Status::unimplemented("set_use_twitter_image not implemented"))
    }

    async fn request_random_proof(&self, _request: Request<RequestRandomProofRequest>) -> Result<Response<RequestRandomProofResponse>, Status> {
        Err(Status::unimplemented("request_random_proof not implemented"))
    }

    async fn submit_captcha_game(&self, _request: Request<SubmitCaptchaGameRequest>) -> Result<Response<SubmitCaptchaGameResponse>, Status> {
        Err(Status::unimplemented("submit_captcha_game not implemented"))
    }

    async fn redeem_stars(&self, _request: Request<RedeemStarsRequest>) -> Result<Response<RedeemStarsResponse>, Status> {
        Err(Status::unimplemented("redeem_stars not implemented"))
    }

    async fn get_flappy_leaderboard(&self, _request: Request<GetFlappyLeaderboardRequest>) -> Result<Response<GetFlappyLeaderboardResponse>, Status> {
        Err(Status::unimplemented("get_flappy_leaderboard not implemented"))
    }

    async fn set_turbo_high_score(&self, _request: Request<SetTurboHighScoreRequest>) -> Result<Response<SetTurboHighScoreResponse>, Status> {
        Err(Status::unimplemented("set_turbo_high_score not implemented"))
    }

    async fn submit_quiz_game(&self, _request: Request<SubmitQuizGameRequest>) -> Result<Response<SubmitQuizGameResponse>, Status> {
        Err(Status::unimplemented("submit_quiz_game not implemented"))
    }

    async fn get_turbo_leaderboard(&self, _request: Request<GetTurboLeaderboardRequest>) -> Result<Response<GetTurboLeaderboardResponse>, Status> {
        Err(Status::unimplemented("get_turbo_leaderboard not implemented"))
    }

    async fn submit_eth_block_metadata(&self, _request: Request<SubmitEthBlockMetadataRequest>) -> Result<Response<SubmitEthBlockMetadataResponse>, Status> {
        Err(Status::unimplemented("submit_eth_block_metadata not implemented"))
    }

    async fn get_filtered_eth_block_requests(&self, _request: Request<GetFilteredEthBlockRequestsRequest>) -> Result<Response<GetFilteredEthBlockRequestsResponse>, Status> {
        Err(Status::unimplemented("get_filtered_eth_block_requests not implemented"))
    }

    async fn set2048_high_score(&self, _request: Request<Set2048HighScoreRequest>) -> Result<Response<Set2048HighScoreResponse>, Status> {
        Err(Status::unimplemented("set2048_high_score not implemented"))
    }

    async fn set_volleyball_high_score(&self, _request: Request<SetVolleyballHighScoreRequest>) -> Result<Response<SetVolleyballHighScoreResponse>, Status> {
        Err(Status::unimplemented("set_volleyball_high_score not implemented"))
    }

    async fn get_eth_block_request_metrics(&self, _request: Request<GetEthBlockRequestMetricsRequest>) -> Result<Response<GetEthBlockRequestMetricsResponse>, Status> {
        Err(Status::unimplemented("get_eth_block_request_metrics not implemented"))
    }

    async fn set_turbo_time_trial_high_score(&self, _request: Request<SetTurboTimeTrialHighScoreRequest>) -> Result<Response<SetTurboTimeTrialHighScoreResponse>, Status> {
        Err(Status::unimplemented("set_turbo_time_trial_high_score not implemented"))
    }

    async fn set_coin_craze_high_score(&self, _request: Request<SetCoinCrazeHighScoreRequest>) -> Result<Response<SetCoinCrazeHighScoreResponse>, Status> {
        Err(Status::unimplemented("set_coin_craze_high_score not implemented"))
    }

    async fn set_lean_high_score(&self, _request: Request<SetLeanHighScoreRequest>) -> Result<Response<SetLeanHighScoreResponse>, Status> {
        Err(Status::unimplemented("set_lean_high_score not implemented"))
    }

    async fn set_flow_high_score(&self, _request: Request<SetFlowHighScoreRequest>) -> Result<Response<SetFlowHighScoreResponse>, Status> {
        Err(Status::unimplemented("set_flow_high_score not implemented"))
    }

    async fn set_rollup_high_score(&self, _request: Request<SetRollupHighScoreRequest>) -> Result<Response<SetRollupHighScoreResponse>, Status> {
        Err(Status::unimplemented("set_rollup_high_score not implemented"))
    }

    async fn get_pending_stars(&self, _request: Request<GetPendingStarsRequest>) -> Result<Response<GetPendingStarsResponse>, Status> {
        Err(Status::unimplemented("get_pending_stars not implemented"))
    }

    async fn get_whitelist_status(&self, _request: Request<GetWhitelistStatusRequest>) -> Result<Response<GetWhitelistStatusResponse>, Status> {
        Err(Status::unimplemented("get_whitelist_status not implemented"))
    }

    async fn claim_gpu(&self, _request: Request<ClaimGpuRequest>) -> Result<Response<ClaimGpuResponse>, Status> {
        Err(Status::unimplemented("claim_gpu not implemented"))
    }

    async fn set_gpu_variant(&self, _request: Request<SetGpuVariantRequest>) -> Result<Response<SetGpuVariantResponse>, Status> {
        Err(Status::unimplemented("set_gpu_variant not implemented"))
    }

    async fn link_whitelisted_twitter(&self, _request: Request<LinkWhitelistedTwitterRequest>) -> Result<Response<LinkWhitelistedTwitterResponse>, Status> {
        Err(Status::unimplemented("link_whitelisted_twitter not implemented"))
    }

    async fn retrieve_proving_key(&self, _request: Request<RetrieveProvingKeyRequest>) -> Result<Response<RetrieveProvingKeyResponse>, Status> {
        Err(Status::unimplemented("retrieve_proving_key not implemented"))
    }

    async fn link_whitelisted_github(&self, _request: Request<LinkWhitelistedGithubRequest>) -> Result<Response<LinkWhitelistedGithubResponse>, Status> {
        Err(Status::unimplemented("link_whitelisted_github not implemented"))
    }

    async fn link_whitelisted_discord(&self, _request: Request<LinkWhitelistedDiscordRequest>) -> Result<Response<LinkWhitelistedDiscordResponse>, Status> {
        Err(Status::unimplemented("link_whitelisted_discord not implemented"))
    }

    async fn get_prover_leaderboard(&self, _request: Request<GetProverLeaderboardRequest>) -> Result<Response<GetProverLeaderboardResponse>, Status> {
        Err(Status::unimplemented("get_prover_leaderboard not implemented"))
    }

    async fn get_filtered_gpus(&self, _request: Request<GetFilteredGpusRequest>) -> Result<Response<GetFilteredGpusResponse>, Status> {
        Err(Status::unimplemented("get_filtered_gpus not implemented"))
    }

    async fn set_gpu_coordinates(&self, _request: Request<SetGpuCoordinatesRequest>) -> Result<Response<SetGpuCoordinatesResponse>, Status> {
        Err(Status::unimplemented("set_gpu_coordinates not implemented"))
    }

    async fn get_points(&self, _request: Request<GetPointsRequest>) -> Result<Response<GetPointsResponse>, Status> {
        Err(Status::unimplemented("get_points not implemented"))
    }

    async fn process_clicks(&self, _request: Request<ProcessClicksRequest>) -> Result<Response<ProcessClicksResponse>, Status> {
        Err(Status::unimplemented("process_clicks not implemented"))
    }

    async fn purchase_upgrade(&self, _request: Request<PurchaseUpgradeRequest>) -> Result<Response<PurchaseUpgradeResponse>, Status> {
        Err(Status::unimplemented("purchase_upgrade not implemented"))
    }

    async fn bet(&self, _request: Request<BetRequest>) -> Result<Response<BetResponse>, Status> {
        Err(Status::unimplemented("bet not implemented"))
    }

    async fn get_contest_details(&self, _request: Request<GetContestDetailsRequest>) -> Result<Response<GetContestDetailsResponse>, Status> {
        Err(Status::unimplemented("get_contest_details not implemented"))
    }

    async fn get_latest_contest(&self, _request: Request<GetLatestContestRequest>) -> Result<Response<GetLatestContestResponse>, Status> {
        Err(Status::unimplemented("get_latest_contest not implemented"))
    }

    async fn get_contest_bettors(&self, _request: Request<GetContestBettorsRequest>) -> Result<Response<GetContestBettorsResponse>, Status> {
        Err(Status::unimplemented("get_contest_bettors not implemented"))
    }

    async fn get_gpu_metrics(&self, _request: Request<GetGpuMetricsRequest>) -> Result<Response<GetGpuMetricsResponse>, Status> {
        Err(Status::unimplemented("get_gpu_metrics not implemented"))
    }

    async fn get_filtered_prover_activity(&self, _request: Request<GetFilteredProverActivityRequest>) -> Result<Response<GetFilteredProverActivityResponse>, Status> {
        Err(Status::unimplemented("get_filtered_prover_activity not implemented"))
    }

    async fn get_prover_metrics(&self, _request: Request<GetProverMetricsRequest>) -> Result<Response<GetProverMetricsResponse>, Status> {
        Err(Status::unimplemented("get_prover_metrics not implemented"))
    }

    async fn get_filtered_bet_history(&self, _request: Request<GetFilteredBetHistoryRequest>) -> Result<Response<GetFilteredBetHistoryResponse>, Status> {
        Err(Status::unimplemented("get_filtered_bet_history not implemented"))
    }

    async fn get_gpu_team_stats(&self, _request: Request<GetGpuTeamStatsRequest>) -> Result<Response<GetGpuTeamStatsResponse>, Status> {
        Err(Status::unimplemented("get_gpu_team_stats not implemented"))
    }

    async fn get_config_values(&self, _request: Request<GetConfigValuesRequest>) -> Result<Response<GetConfigValuesResponse>, Status> {
        Err(Status::unimplemented("get_config_values not implemented"))
    }

    async fn get_prover_stats(&self, _request: Request<GetProverStatsRequest>) -> Result<Response<GetProverStatsResponse>, Status> {
        Err(Status::unimplemented("get_prover_stats not implemented"))
    }

    async fn get_filtered_prover_stats(&self, _request: Request<GetFilteredProverStatsRequest>) -> Result<Response<GetFilteredProverStatsResponse>, Status> {
        Err(Status::unimplemented("get_filtered_prover_stats not implemented"))
    }

    async fn get_prover_stats_detail(&self, _request: Request<GetProverStatsDetailRequest>) -> Result<Response<GetProverStatsDetailResponse>, Status> {
        Err(Status::unimplemented("get_prover_stats_detail not implemented"))
    }

    async fn get_prover_search_results(&self, _request: Request<GetProverSearchResultsRequest>) -> Result<Response<GetProverSearchResultsResponse>, Status> {
        Err(Status::unimplemented("get_prover_search_results not implemented"))
    }

    async fn get_filtered_bid_history(&self, _request: Request<GetFilteredBidHistoryRequest>) -> Result<Response<GetFilteredBidHistoryResponse>, Status> {
        Err(Status::unimplemented("get_filtered_bid_history not implemented"))
    }

    async fn get_tee_whitelist_status(&self, _request: Request<GetTeeWhitelistStatusRequest>) -> Result<Response<GetTeeWhitelistStatusResponse>, Status> {
        Err(Status::unimplemented("get_tee_whitelist_status not implemented"))
    }

    async fn get_settlement_request(&self, _request: Request<GetSettlementRequestRequest>) -> Result<Response<GetSettlementRequestResponse>, Status> {
        Err(Status::unimplemented("get_settlement_request not implemented"))
    }

    async fn get_filtered_settlement_requests(&self, _request: Request<GetFilteredSettlementRequestsRequest>) -> Result<Response<GetFilteredSettlementRequestsResponse>, Status> {
        Err(Status::unimplemented("get_filtered_settlement_requests not implemented"))
    }

    async fn get_filtered_provers(&self, _request: Request<GetFilteredProversRequest>) -> Result<Response<GetFilteredProversResponse>, Status> {
        Err(Status::unimplemented("get_filtered_provers not implemented"))
    }

    async fn get_prover_stake_balance(&self, _request: Request<GetProverStakeBalanceRequest>) -> Result<Response<GetProverStakeBalanceResponse>, Status> {
        Err(Status::unimplemented("get_prover_stake_balance not implemented"))
    }

    async fn get_filtered_staker_stake_balance_logs(&self, _request: Request<GetFilteredStakerStakeBalanceLogsRequest>) -> Result<Response<GetFilteredStakerStakeBalanceLogsResponse>, Status> {
        Err(Status::unimplemented("get_filtered_staker_stake_balance_logs not implemented"))
    }

    async fn get_filtered_prover_stake_balance_logs(&self, _request: Request<GetFilteredProverStakeBalanceLogsRequest>) -> Result<Response<GetFilteredProverStakeBalanceLogsResponse>, Status> {
        Err(Status::unimplemented("get_filtered_prover_stake_balance_logs not implemented"))
    }

    async fn get_delegation_params(&self, _request: Request<GetDelegationParamsRequest>) -> Result<Response<GetDelegationParamsResponse>, Status> {
        Err(Status::unimplemented("get_delegation_params not implemented"))
    }

    async fn set_delegation(&self, _request: Request<SetDelegationRequest>) -> Result<Response<SetDelegationResponse>, Status> {
        Err(Status::unimplemented("set_delegation not implemented"))
    }

    async fn get_delegation(&self, _request: Request<GetDelegationRequest>) -> Result<Response<GetDelegationResponse>, Status> {
        Err(Status::unimplemented("get_delegation not implemented"))
    }

    async fn get_filtered_withdrawal_receipts(&self, _request: Request<GetFilteredWithdrawalReceiptsRequest>) -> Result<Response<GetFilteredWithdrawalReceiptsResponse>, Status> {
        Err(Status::unimplemented("get_filtered_withdrawal_receipts not implemented"))
    }
}

const PROTOS: &[u8] = include_bytes!("../../crates/types/rpc/src/generated/descriptor.bin");

/// Run a real gRPC server using tonic
pub async fn run_server(mut shutdown_rx: mpsc::Receiver<()>) -> Result<()> {
    println!("=== Starting gRPC Server ===");
    
    let addr = "127.0.0.1:50051".parse()?;
    let service = ProverNetworkServiceImpl::default();
    
    // build a descriptor set at compile-time with prost-build / tonic-prost-build
    // then include it here (PROTOS is &[u8])
    let reflection = ReflBuilder::configure()
        .register_encoded_file_descriptor_set(PROTOS)
        .build_v1()?;

    println!("ProverNetwork gRPC Server listening on {}", addr);
    println!("Server will run until shutdown signal is received...");
    
    // Create a real tonic gRPC server
    let server = Server::builder()
        .add_service(prover_network_server::ProverNetworkServer::new(service))
        .add_service(reflection)
        .serve_with_shutdown(addr, async {
            shutdown_rx.recv().await;
            println!("Shutdown signal received, gracefully stopping server...");
        });
    
    if let Err(e) = server.await {
        eprintln!("Server error: {}", e);
    }
    
    println!("Server shutdown complete");
    Ok(())
}
