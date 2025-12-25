# Feature Specification: Plugin Enhancements

**Feature Branch**: `007-plugin-enhancements`
**Created**: 2025-12-25
**Status**: Draft
**Input**: User description: "1.基于 redis 的限流 2.外部存储 比如数据库的访问"

## User Scenarios & Testing

### User Story 1 - Redis-based Rate Limiting (Priority: P1)

As a plugin developer, I want to implement rate limiting logic using a shared Redis instance so that I can enforce global rate limits across multiple gateway instances.

**Why this priority**: Essential for protecting backend services in a distributed environment.

**Independent Test**:

- Deploy AGW with valid Redis config.
- Deploy a Wasm plugin that calls the Redis host function to increment and check counters.
- Send requests exceeding the limit and verify 429 responses.

**Acceptance Scenarios**:

1. **Given** a configured Redis connection in AGW, **When** a plugin requests a rate limit check (e.g., `incr` + `expire`), **Then** AGW executes the command on Redis and returns the result to Wasm.
2. **Given** Redis is unavailable, **When** a plugin requests a check, **Then** the plugin receives an error and can choose to fail open or closed.

---

### User Story 2 - External Database Access (Priority: P2)

As a plugin developer, I want to query an external database (e.g., MySQL, Postgres) to fetch dynamic configuration or authentication data during request processing.

**Why this priority**: Enables complex logic like dynamic routing based on user tier or advanced authentication.

**Independent Test**:

- Deploy AGW with DB config.
- Deploy a plugin that executes a SELECT query.
- Verify the plugin receives the query results and acts on them.

**Acceptance Scenarios**:

1. **Given** a SQL query from Wasm, **When** executed via host function, **Then** the result set is serialized and returned to Wasm.
2. **Given** a slow DB query, **When** the timeout is reached, **Then** the plugin execution is aborted or returns an error.

## Requirements

### Functional Requirements

- **FR-001**: The System MUST provide Wasm host functions to perform Redis commands (at least `INCR`, `GET`, `SET`, `EXPIRE`).
- **FR-002**: The System MUST provide Wasm host functions to execute SQL queries on configured external databases.
- **FR-003**: The Control Plane MUST allow configuring external resources (Redis/DB connections) and passing these configs (or connection pools) to the Data Plane.
- **FR-004**: The Data Plane MUST manage connection pools to these external resources efficiently, shared across Wasm VM instances.
- **FR-005**: All external calls MUST be non-blocking to the main event loop (using async Rust host functions).

### Key Entities

- **ExternalResource**: Configuration for an external service (Redis/DB) including connection string, credentials, and pool settings.
- **HostFunctionContext**: The context passed to Wasm usually needs to hold or access these connection pools.

## Success Criteria

### Measurable Outcomes

- **SC-001**: Redis rate limit check overhead < 2ms (excluding network latency).
- **SC-002**: Support at least 1000 concurrent plugin instances performing external calls.
