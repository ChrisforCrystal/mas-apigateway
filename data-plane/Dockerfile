FROM rust:1.83-bookworm as builder

WORKDIR /usr/src/app

# Install protobuf compiler
RUN apt-get update && apt-get install -y protobuf-compiler cmake && rm -rf /var/lib/apt/lists/*

# Copy workspace files
# We need proto folder which is outside data-plane
COPY proto /usr/src/proto

# Copy data-plane
COPY data-plane /usr/src/app/data-plane

# Build
WORKDIR /usr/src/app/data-plane
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
