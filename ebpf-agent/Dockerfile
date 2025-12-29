FROM rustlang/rust:nightly as builder

# Install bpf-linker dependency
RUN cargo install bpf-linker --version 0.9.10

# Install rust-src for build-std
RUN rustup component add rust-src

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
RUN cargo build --release --package ebpf-agent-ebpf --target=bpfel-unknown-none -Z build-std=core

# Copy BPF binary to agent source for easy inclusion
# Source: target/bpfel-unknown-none/release/ebpf-agent (binary name is ebpf-agent)
# Dest: agent/src/ebpf-program
RUN cp target/bpfel-unknown-none/release/ebpf-agent agent/src/ebpf-program

# Build User Agent
RUN cargo build --release --package agent

FROM debian:bookworm-slim
# Install utils for debugging
RUN apt-get update && apt-get install -y ca-certificates iproute2 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/ebpf-agent/target/release/ebpf-agent /usr/local/bin/ebpf-agent
# We don't need to copy the BPF file separately if we embedded it with include_bytes_aligned!

CMD ["/usr/local/bin/ebpf-agent", "--cgroup", "/sys/fs/cgroup"]
