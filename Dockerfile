# Build stage - use latest Rust
FROM rust:latest AS builder

WORKDIR /app

# Install protobuf compiler
RUN apt-get update && apt-get install -y protobuf-compiler && rm -rf /var/lib/apt/lists/*

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Create dummy src to cache dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs && echo "pub fn placeholder() {}" > src/lib.rs

# Build dependencies (this layer will be cached)
RUN cargo build --release && rm -rf src

# Copy actual source code
COPY src ./src

# Touch main.rs to force rebuild
RUN touch src/main.rs && touch src/lib.rs

# Build the actual application
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Copy binary from builder
COPY --from=builder /app/target/release/otterwatch-mqtt /app/otterwatch-mqtt

# Copy default config
COPY settings.toml.example /app/settings.toml

# Create data directory
RUN mkdir -p /app/data

# Expose ports
EXPOSE 1883 8085 9090

# Set environment variables
ENV RUST_LOG=info

# Run the broker
CMD ["/app/otterwatch-mqtt"]
