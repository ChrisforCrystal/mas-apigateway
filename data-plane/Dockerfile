FROM rust:bookworm as builder

WORKDIR /usr/src/app

# Install protobuf compiler
RUN apt-get update && apt-get install -y protobuf-compiler cmake && rm -rf /var/lib/apt/lists/*

# Copy workspace files
# We need proto folder which is outside data-plane
COPY proto /usr/src/app/proto

# Optimization: Cache dependencies
WORKDIR /usr/src/app/data-plane
COPY data-plane/Cargo.toml data-plane/build.rs ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release

# Build actual app
COPY data-plane/src ./src
# Update timestamp to force rebuild of main.rs
RUN touch src/main.rs
RUN cargo build --release

# Runtime image
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y openssl ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/app/data-plane/target/release/data-plane /usr/local/bin/data-plane

# Expose ports
EXPOSE 6188 6443

# We need to set RUST_LOG
ENV RUST_LOG=debug

CMD ["data-plane"]
