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
# Update timestamp to force rebuild of main.rs
RUN touch src/main.rs
RUN cargo build --release

# --- Build Plugins (New Stage) ---
# We need to install the wasm32 target
RUN rustup target add wasm32-unknown-unknown

# Copy plugins source
COPY plugins /usr/src/app/plugins

# Build deny-all plugin
WORKDIR /usr/src/app/plugins/deny-all
RUN cargo build --target wasm32-unknown-unknown --release

# Build redis-demo plugin
WORKDIR /usr/src/app/plugins/redis-demo
RUN cargo build --target wasm32-unknown-unknown --release

# Build db-demo plugin
WORKDIR /usr/src/app/plugins/db-demo
RUN cargo build --target wasm32-unknown-unknown --release

# Runtime image
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y openssl ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/app/data-plane/target/release/data-plane /usr/local/bin/data-plane

# Copy compiled plugins to a known location
RUN mkdir -p /etc/mas-agw/plugins
COPY --from=builder /usr/src/app/plugins/deny-all/target/wasm32-unknown-unknown/release/deny_all.wasm /etc/mas-agw/plugins/deny_all.wasm
COPY --from=builder /usr/src/app/plugins/redis-demo/target/wasm32-unknown-unknown/release/redis_demo.wasm /etc/mas-agw/plugins/redis_demo.wasm
COPY --from=builder /usr/src/app/plugins/db-demo/target/wasm32-unknown-unknown/release/db_demo.wasm /etc/mas-agw/plugins/db_demo.wasm

# Expose ports
EXPOSE 6188 6443

# We need to set RUST_LOG
ENV RUST_LOG=debug

CMD ["data-plane"]
