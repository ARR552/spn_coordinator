FROM rust:1.88 AS builder

RUN apt-get update && apt-get install -y \
    build-essential \
    protobuf-compiler \
    pkg-config
    

WORKDIR /build

COPY . .

RUN cargo build --bin spn_coordinator --release && \
    cp target/release/spn_coordinator /build/spn_coordinator

FROM rust:1.88-slim

WORKDIR /app

# Install required runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy only the built binaries from builder
COPY --from=builder /build/spn_coordinator /usr/local/bin/spn_coordinator
COPY --from=builder /build/testing-cert /app/testing-cert

# Run the server from its permanent location
CMD ["/usr/local/bin/spn_coordinator"]