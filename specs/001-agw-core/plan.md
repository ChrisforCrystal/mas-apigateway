# Implementation Plan: AGW Core Initialization

**Branch**: `001-agw-core` | **Date**: 2025-12-21 | **Spec**: [specs/001-agw-core/spec.md](spec.md)
**Input**: Feature specification from `/specs/001-agw-core/spec.md`

## Summary

Initialize the "AGW" project with a Monorepo structure enforcing separation between the Go Control Plane and Rust Data Plane. Establish the core gRPC communication channel using `tonic` (Rust) and `grpc-go` (Go), and bootstrap a basic Pingora server instance.

## Technical Context

**Language/Version**: Rust 2021, Go 1.22+
**Primary Dependencies**:

- Data Plane: `pingora` (Proxy Engine), `tonic`/`prost` (gRPC), `tokio` (Runtime).
- Control Plane: `google.golang.org/grpc`, `cobra`/`viper` (CLI/Config).
  **Storage**: N/A for this phase (in-memory config distribution).
  **Testing**: `cargo test`, `go test`.
  **Target Platform**: Linux (production), macOS (dev).
  **Project Type**: Monorepo (Hybrid Go/Rust).
  **Performance Goals**: N/A for init, but architecture must support zero-copy.
  **Constraints**: Strict adherence to Constitution Iron Laws.

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

- [x] **Architecture Iron Law**: CP/DP separated via gRPC? **YES**.
- [x] **Safety Iron Law**: No business logic in DP? **YES** (DP is purely a proxy engine connection).
- [x] **Performance Iron Law**: Using Pingora? **YES**.
- [x] **Directory Structure**: Matches Section 3? **YES**.
- [x] **Tech Stack**: Matches Section 2? **YES**.

## Project Structure

### Documentation (this feature)

```text
specs/001-agw-core/
├── plan.md              # This file
├── research.md          # Technology decisions
├── data-model.md        # Concept definitions
├── quickstart.md        # Build/Run instructions
├── contracts/           # API definitions
│   └── agw.proto        # Core gRPC service
└── tasks.md             # Execution steps
```

### Source Code (repository root)

```text
agw/
├── .specify/            # Spec Kit
├── constitution.md      # Governance
├── cli/                 # [NEW] Rust CLI workspace member
│   └── Cargo.toml
├── proto/               # [NEW] Protocol definitions
│   └── agw.proto
├── control-plane/       # [NEW] Go module
│   ├── cmd/
│   ├── internal/server/
│   ├── pkg/
│   └── go.mod
├── data-plane/          # [NEW] Rust workspace member
│   ├── src/
│   └── Cargo.toml
├── plugins/             # [NEW] Placeholder directory
└── deploy/              # [NEW] Placeholder directory
```

**Structure Decision**: Monorepo as defined in Constitution Section 3.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

| Violation | Why Needed | Simpler Alternative Rejected Because |
| --------- | ---------- | ------------------------------------ |
| None      | N/A        | N/A                                  |
