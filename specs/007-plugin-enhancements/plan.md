# Implementation Plan: Plugin Enhancements

**Branch**: `007-plugin-enhancements` | **Date**: 2025-12-25 | **Spec**: [spec.md](file:///Users/jiwn2/dev/masallsome/masapigateway/specs/007-plugin-enhancements/spec.md)
**Input**: Feature specification from `/specs/007-plugin-enhancements/spec.md`

**Note**: This template is filled in by the `/speckit.plan` command. See `.specify/templates/commands/plan.md` for the execution workflow.

## Summary

Enhance the Wasm plugin system in the Data Plane to support external network calls, specifically for Redis-based rate limiting and external SQL database access. This involves adding new host functions to the Wasmtime runtime and configuring connection pools in the Data Plane.

## Technical Context

**Language/Version**: Rust 2021 (Data Plane), Go 1.22+ (Control Plane).
**Primary Dependencies**: `wasmtime` (existing). New: `redis` (Rust), `sqlx` or `tokio-postgres`/`mysql_async` (Rust) for DB access.
**Storage**: External Redis and SQL Databases.
**Testing**: `cargo test` for host functions, integration tests with `testcontainers` or mocked services.
**Target Platform**: Linux/K8s (Data Plane).
**Performance Goals**: Low latency for rate checks (<2ms added overhead). Non-blocking async execution is critical.
**Constraints**: Wasm guests are single-threaded (mostly). Host functions must be async and thread-safe.
**Scale/Scope**: Support high concurrency.

### Unknowns & Clarifications (Phase 0)

1. **Host Function Interface**: defining the exact signature for `redis_command` and `sql_query` in Wasm.
2. **Async Integration**: How to properly await Rust async futures from within Wasmtime host functions? (Wasmtime has async support, need to verify implementation details).
3. **Configuration**: How to inject Redis/DB credentials from Control Plane (xDS) to Data Plane.

## Constitution Check

_GATE: Must pass before Phase 0 research. Re-check after Phase 1 design._

- **Modular**: Yes, extends existing Wasm plugin system.
- **Performant**: Must be async/non-blocking.
- **Secure**: Credentials management (K8s Secrets -> Control Plane -> Data Plane).

## Project Structure

### Documentation (this feature)

```text
specs/007-plugin-enhancements/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── contracts/           # Phase 1 output
└── tasks.md             # Phase 2 output
```

### Source Code

```text
data-plane/
├── src/
│   ├── plugins/
│   │   ├── host_functions.rs  # [MODIFY] Add new host functions
│   │   ├── context.rs         # [MODIFY] Add Redis/DB pools to context
│   │   └── runtime.rs         # [MODIFY] Register new host functions
│   └── main.rs                # [MODIFY] Initialize connection pools
```
