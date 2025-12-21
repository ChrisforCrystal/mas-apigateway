# Research: AGW Core Initialization

**Feature**: AGW Core Setup (001-agw-core)
**Date**: 2025-12-21

## Technology Decisions

### 1. Versions

- **Rust**: 2021 Edition (Constitution Requirement).
- **Go**: 1.22+ (Constitution Requirement).
- **Pingora**: Latest stable (0.1.x or newer).
- **Tonic**: Latest stable compatible with Pingora.
- **gRPC-Go**: Latest stable (1.6x).

### 2. Protocol Buffers Structure

- **Decision**: Use a simplified xDS model.
- **Rationale**: Full Envoy xDS is too complex for an internal AGW. We need a push-based model where Control Plane streams updates to Data Plane.
- **Service**: `AgwService`
  - `StreamConfig(Node)` -> `stream ConfigSnapshot`

### 3. Build Tools

- **Codegen**: use `protoc` directly via `build.rs` `prost-build` (Data Plane) and `protoc-gen-go` (Control Plane).
- **Justification**: Keeps build hermetic where possible, though `protoc` binary is expected on host.

## Constitution Compliance Check

| Constraint                    | Check | Notes                                                     |
| ----------------------------- | ----- | --------------------------------------------------------- |
| Iron Law 1 (CP/DP Separation) | ✅    | gRPC streaming architecture chosen.                       |
| Iron Law 2 (Wasm Safety)      | N/A   | Feature is initialization only; Wasm runtime comes later. |
| Iron Law 3 (Zero Copy)        | ✅    | Pingora selected as engine.                               |
| Tech Stack (Rust/Go)          | ✅    | Monorepo structure enforces this.                         |
