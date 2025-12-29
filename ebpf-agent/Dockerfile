FROM rust:1.83 as builder

# Install bpf-linker dependency
RUN cargo install bpf-linker

WORKDIR /app

# Copy workspace
COPY Cargo.toml Cargo.lock ./
COPY ebpf-agent ./ebpf-agent
COPY data-plane ./data-plane
# Note: Copied data-plane just in case workspace resolver needs it, though likely not for ebpf-agent alone if structured right.
# Better to focus on ebpf-agent dir context.

# Force isolated build for ebpf-agent
WORKDIR /app/ebpf-agent

# Build Kernel (BPF)
# Note: Target dir will be inside ebpf-agent/target
RUN cargo build --release --pkgs ebpf-agent-ebpf --target=bpfel-unknown-none -Z build-std=core

# Build User Agent
RUN cargo build --release --pkgs agent

FROM debian:bookworm-slim
# Install utils for debugging
RUN apt-get update && apt-get install -y ca-certificates iproute2 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/ebpf-agent/target/release/ebpf-agent /usr/local/bin/ebpf-agent
# We don't need to copy the BPF file separately if we embedded it with include_bytes_aligned!
# But wait, our main.rs logic used `include_bytes_aligned!("../../target/...")`.
# In Docker build context, that path must be valid during `cargo build`. 
# The builder stage handles this.

CMD ["/usr/local/bin/ebpf-agent", "--cgroup", "/sys/fs/cgroup"]
