---
description: "Tasks for Plugin Enhancements (Redis + DB)"
---

# Tasks: Plugin Enhancements

**Input**: Design documents from `/specs/007-plugin-enhancements/`
**Prerequisites**: plan.md, spec.md, research.md

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Update dependencies for Data Plane and Control Plane.

- [x] T001 [P] Add Rust dependencies to `data-plane/Cargo.toml` (`redis`, `sqlx`, `tokio` features)
- [x] T002 [P] Update `control-plane` Go modules if needed (likely just proto gen)

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Configuration and Connection Management (Blocks both stories)

- [x] T003 Define `RedisConfig` and `DatabaseConfig` in `proto/agw.proto`
- [x] T004 Generate Protobuf code for Rust (`data-plane`) and Go (`control-plane`)
- [x] T005 Implement `ExternalResources` struct in `data-plane/src/plugins/context.rs` to hold pools
- [x] T006 Implement config loading/initialization in `data-plane/src/main.rs` to create pools

## Phase 3: User Story 1 - Redis-based Rate Limiting (Priority: P1) 🎯 MVP

**Goal**: Enable Wasm plugins to execute Redis commands.
**Independent Test**: Verify Wasm can set/get keys in Redis.

### Implementation for User Story 1

- [x] T007 Define host function signature for `host_redis_command`
- [x] T008 [P] Implement async `host_redis_command` in `data-plane/src/plugins/host_functions.rs` (implemented in wasm.rs)
- [x] T009 [P] Register `host_redis_command` in `data-plane/src/plugins/runtime.rs` using `Func::new_async`
- [ ] T010 Add integration test for Redis host function (using `testcontainers` or mock) - _Covered by redis-demo plugin_

## Phase 4: User Story 2 - External Database Access (Priority: P2)

**Goal**: Enable Wasm plugins to execute SQL queries.
**Independent Test**: Verify Wasm can SELECT from DB.

### Implementation for User Story 2

- [x] T011 Define host function signature for `host_db_query`
- [x] T012 [P] Implement async `host_db_query` in `data-plane/src/plugins/host_functions.rs` (implemented in wasm.rs)
- [x] T013 [P] Register `host_db_query` in `data-plane/src/plugins/runtime.rs`
- [ ] T014 Add integration test for DB host function

## Phase 5: Polish & Verification

- [x] T015 Create example Wasm plugin (Rust) demonstrating Redis rate limit (`plugins/redis-demo`)
- [x] T016 Create example Wasm plugin (Rust) demonstrating SQL query in `plugins/db-demo`
- [x] T017 Update `VERIFICATION.md` with new test scenarios
- [ ] T018 [Optimization] Research and prototype Wasm Component Model (WIT) to replace manual pointer handling in `host_functions.rs`
