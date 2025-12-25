# Phase 0: Research Findings

**Feature**: Plugin Enhancements (Redis & DB)
**Date**: 2025-12-25

## 1. Host Function Interface

**Decision**:
Use a simple request/response model where Wasm passes serialized commands/queries and receives serialized results.

- **Redis**: `host_redis_command(cmd: ptr, len: u32) -> Result<ptr, len>`
- **DB**: `host_db_query(query: ptr, len: u32) -> Result<ptr, len>`

However, for better type safety and performance, using `wit-bindgen` or defining strictly typed imports is better, but given the current "host function" style in `masapigateway` (likely using basic `Caller` and memory offsets), we can stick to the existing pattern or improve it.

**Rationale**: keeps compatibility with existing `plugins/` structure.

## 2. Async Integration with Wasmtime

**Decision**:
Enable `Config::async_support(true)` in Wasmtime. Use `Func::new_async` (or `Linker` equivalent) to define host functions.

**Details**:

- The `WasmContext` (host state) will hold `Arc<redis::Client>` and `Arc<sqlx::Pool>`.
- When Wasm calls `host_redis_command`, the host function will:
  1. Read memory from Wasm to get the command.
  2. `await` the Redis operation.
  3. Write the result back to Wasm memory.
  4. Return.
- This requires the `main` loop to drive the Wasm execution asynchronously.

## 3. Configuration (xDS)

**Decision**:
Extend the Protobuf definitions to include `ExternalResources`.

**Protobuf Changes**:
Add `redis_config` and `db_config` to `PluginConfiguration` or a global `GatewayConfiguration`.

```proto
message RedisConfig {
  string address = 1;
  string password = 2;
  int32 db = 3;
}

message DatabaseConfig {
  string connection_string = 1; // e.g. "postgres://user:pass@host/db"
  string type = 2; // "postgres", "mysql"
}
```

The Data Plane will parse this, initialize pools, and store them in the `WasmContext` factory.

## Constants & Limits

- Max concurrent DB connections per instance: Configurable (default 20).
- Redis timeout: 500ms default.
- DB timeout: 1s default.
