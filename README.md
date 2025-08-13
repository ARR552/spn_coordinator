#SPN_Coordinator
This service allow to use sp1 nodes without the need of use the succint infrastructure.

#### Run:
```
cargo build
cargo run
```

### Command to run spn-node:
```
docker run --rm   --network host   --gpus all   -v /var/run/docker.sock:/var/run/docker.sock   -e DOCKER_HOST=unix:///var/run/docker.sock   -e RUST_LOG=debug -e RUST_BACKTRACE=1   public.ecr.aws/succinct-labs/spn-node:latest-gpu prove --rpc-url http://localhost:50051     --throughput 1000     --bid 0   --private-key "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"     --prover "0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266"
```

### Docker build:
```
docker build -t arr551/spn-coordinator .
docker tag arr551/spn-coordinator arr551/spn-coordinator:v0.0.1-dev
```