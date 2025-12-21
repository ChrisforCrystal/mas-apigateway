# Feature Specification: AGW Core Initialization

**Feature Branch**: `001-agw-core`
**Created**: 2025-12-21
**Status**: Draft
**Input**: User description: "Initialize AGW (AI Gateway) project infrastructure based on Constitution v1.0.0"

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Project Scaffolding & Monorepo Setup (Priority: P1)

As a Developer, I want a compliant monorepo structure so that I can begin work on Control and Data planes immediately without violating constitutional directory rules.

**Why this priority**: Iron Laws require strict structure. Foundational for all subsequent work.

**Independent Test**:

1. Run `tree -L 2` and verify exact match with Constitution Section 3.
2. Verify `go.mod` exists in `control-plane/`.
3. Verify `Cargo.toml` exists in `data-plane/` and `cli/`.

**Acceptance Scenarios**:

1. **Given** an empty root (except spec/constitution), **When** initialization completes, **Then** all 7 required top-level directories (`.specify`, `constitution.md`, `cli`, `proto`, `control-plane`, `data-plane`, `plugins`, `deploy`) exist.
2. **Given** the `data-plane` directory, **When** checked, **Then** it contains a valid Rust workspace.
3. **Given** the `control-plane` directory, **When** checked, **Then** it contains a valid Go module.

---

### User Story 2 - Basic Data Plane (Pingora) Startup (Priority: P1)

As a Platform Engineer, I want the Data Plane to compile and start a basic Pingora server so that I can verify the Rust toolchain and dependency on Pingora are correctly configured.

**Why this priority**: Validates the "Muscle" (Constitution Section 2.A).

**Independent Test**:

1. `cd data-plane && cargo run`
2. `curl localhost:6188` (or default pingora port) returns a 404 or Welcome response (proving listener is active).

**Acceptance Scenarios**:

1. **Given** the `data-plane` code, **When** `cargo build` is run, **Then** it compiles without errors using Rust 2021 edition.
2. **Given** the running binary, **When** checking process list, **Then** a process named `agw-data-plane` (or similar) is active.
3. **Given** the server is running, **When** sent a request, **Then** it accepts the connection (even if it returns error/empty).

---

### User Story 3 - Control Plane to Data Plane Communication (Priority: P1)

As a System Architect, I want the Data Plane to connect to the Control Plane via gRPC so that I can verify the "Brain" and "Muscle" are disconnected but communicable (Constitution Iron Law #1).

**Why this priority**: Verifies the core architecture pattern (xDS-like streaming) and Protocol Buffer definitions.

**Independent Test**:

1. Start `control-plane` (server).
2. Start `data-plane` (client).
3. Check logs on Control Plane: "New node connected".
4. Check logs on Data Plane: "Connected to Control Plane".

**Acceptance Scenarios**:

1. **Given** defined `proto/agw.proto`, **When** `protoc` (or script) run, **Then** Go and Rust stubs are generated.
2. **Given** running Control Plane, **When** Data Plane starts, **Then** it establishes a persistent gRPC stream.
3. **Given** the stream, **When** Control Plane sends a "Hello/Config" message, **Then** Data Plane logs receipt.

---

## Requirements _(mandatory)_

### Functional Requirements

- **FR-001**: System MUST be organized as a Monorepo with `control-plane` (Go), `data-plane` (Rust), `proto`, `cli`, `plugins`, and `deploy` directories (Constitution Sec 3).
- **FR-002**: Data Plane MUST rely on `pingora` crate as the core proxy engine (Constitution Sec 2.A).
- **FR-003**: Data Plane MUST use `tonic` for gRPC communication (Constitution Sec 2.A).
- **FR-004**: Control Plane MUST use `google.golang.org/grpc` for serving xDS-like config (Constitution Sec 2.B).
- **FR-005**: Protocol definitions MUST be placed in `proto/` and support generating both Go and Rust code.
- **FR-006**: Data Plane binary MUST be capable of running with a static configuration or flags to locate the Control Plane.
- **FR-007**: Control Plane MUST listen on a TCP port for gRPC connections (default 18000).

### Key Entities

- **Node**: A single Data Plane instance connecting to Control Plane.
- **ConfigSnapshot**: The configuration payload sent from Control Plane to Data Plane (minimal for MVP, e.g., VersionID).

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: `cargo build` in `data-plane` completes in under 5 minutes on standard dev machine.
- **SC-002**: `go build` in `control-plane` completes in under 1 minute.
- **SC-003**: End-to-end latency from Control Plane update to Data Plane log acknowledgement is under 100ms (localhost).
- **SC-004**: Project structure matches Constitution 100% (checked by script/cli).
