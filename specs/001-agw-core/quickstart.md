# Quickstart: AGW Core Initialization

## Prerequisites

- **Rust**: 2021 Edition (`rustup default stable`)
- **Go**: 1.22+ (`go version`)
- **Protoc**: Protocol Buffer Compiler (`protoc --version`)

## Build

### Data Plane (Rust)

```bash
cd data-plane
cargo build
```

### Control Plane (Go)

```bash
cd control-plane
go mod tidy
go build ./...
```

### CLI (Rust)

```bash
cd cli
cargo build
```

## Run

### 1. Start Control Plane

```bash
# Terminal 1
./control-plane/control-plane serve
```

### 2. Start Data Plane

```bash
# Terminal 2
./data-plane/target/debug/agw-data-plane --cp-addr localhost:18000
```
