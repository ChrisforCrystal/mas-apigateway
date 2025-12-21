---
description: "Task list for AGW Core Initialization (001-agw-core)"
---

# Tasks: AGW Core Initialization

**Input**: Design documents from `/specs/001-agw-core/`
**Prerequisites**: plan.md, spec.md, contracts/agw.proto
**Organization**: Tasks are grouped by user story (US1, US2, US3).

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Project root structure and shared protocol definitions.

- [ ] T001 Create git-compliant monorepo directory structure (cli, proto, control-plane, data-plane, plugins, deploy)
- [ ] T002 Create `proto/agw.proto` with AgwService definition in `proto/agw.proto`
- [ ] T003 [P] Create `.gitignore` for Rust and Go projects in root

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core infrastructure MUST be complete before user stories can operate effectively.

- [ ] T004 Install/Verify `protoc` and `protoc-gen-go` availability (checked via script or manual)

---

## Phase 3: User Story 1 - Project Scaffolding & Monorepo Setup (Priority: P1)

**Goal**: Compliant monorepo structure ready for code.
**Independent Test**: `tree` matches Constitution, `go.mod` and `Cargo.toml` exist.

### Implementation for US1

- [ ] T005 [US1] Initialize Go module in `control-plane/` (go mod init)
- [ ] T006 [US1] Initialize Rust workspace in `data-plane/Cargo.toml`
- [ ] T007 [P] [US1] Initialize CLI Rust project in `cli/Cargo.toml` and add to workspace
- [ ] T008 [P] [US1] Create placeholder `plugins` and `deploy` directories with READMEs
- [ ] T009 [US1] Verify directory structure against Constitution using `tree`

**Checkpoint**: Monorepo structure validation passed.

---

## Phase 4: User Story 2 - Basic Data Plane (Pingora) Startup (Priority: P1)

**Goal**: Data Plane compiles and runs a basic Pingora server.
**Independent Test**: `cargo run` and `curl` response.

### Implementation for US2

- [ ] T010 [US2] Add `pingora` and `tokio` dependencies to `data-plane/Cargo.toml`
- [ ] T011 [US2] Implement basic struct implementing `pingora::server::ServerApp` in `data-plane/src/main.rs`
- [ ] T012 [US2] Add logic to start Pingora server listening on port 6188 in `data-plane/src/main.rs`
- [ ] T013 [US2] Verify `cargo build` succeeds
- [ ] T014 [US2] Manual Verification: Run server and `curl localhost:6188`

**Checkpoint**: Data Plane binary runs Pingora engine.

---

## Phase 5: User Story 3 - Control Plane to Data Plane Communication (Priority: P1)

**Goal**: gRPC Streaming connection established (Node -> ConfigSnapshot).
**Independent Test**: Logs on both sides confirming connection.

### Implementation for US3

- [ ] T015 [US3] Add `tonic`, `prost` to `data-plane/Cargo.toml` and `tonic-build` to `data-plane/build.rs`
- [ ] T016 [US3] Configure `data-plane/build.rs` to compile `proto/agw.proto`
- [ ] T017 [US3] Add `google.golang.org/grpc` and `google.golang.org/protobuf` to `control-plane/go.mod`
- [ ] T018 [US3] Create Go codegen script or Makefile entry to generate proto stubs in `control-plane/pkg/proto/`
- [ ] T019 [US3] Implement `AgwService` server in `control-plane/internal/server/grpc.go`
- [ ] T020 [US3] Implement Go main entrypoint starting the gRPC server in `control-plane/cmd/server/main.go`
- [ ] T021 [US3] Implement `AgwService` client connecting to CP in `data-plane/src/client.rs` (or module)
- [ ] T022 [US3] Integrate client startup into `data-plane/src/main.rs` (connect before/during Pingora start)
- [ ] T023 [US3] Add "Connected" logging to both CP and DP
- [ ] T024 [US3] Manual Verification: Start CP, start DP, check logs

**Checkpoint**: End-to-end gRPC stream active.

---

## Validation Strategy

1. **US1**: `tree` output comparison.
2. **US2**: `curl` against Pingora port.
3. **US3**: Log output "New node connected" (CP) and "Config stream established" (DP).

## Dependencies & Execution Order

- **Phase 1 & 2** are strictly serial.
- **US1** blocks US2 and US3.
- **US2** (basic server/dependencies) blocks US3 (integration).
- **US3** requires US1 and US2.
