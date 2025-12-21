# Feature 004: Wasm Plugin Support

## 1. Background & Goal

To make AGW truly extensible, we need to allow users to inject custom logic (Authentication, Rate Limiting, Header Transformation) without recompiling the Data Plane. WebAssembly (Wasm) is the cloud-native standard for this.

**Goal**: Integrate a Wasm Runtime (Wasmtime) into the Data Plane to execute user-defined plugins during the request lifecycle.

## 2. Technical Architecture

### Data Plane (Rust)

- **Runtime**: Use `wasmtime` crate for embedding Wasm.
- **Execution Point**: Inside `request_filter` phase of Pingora.
- **Lifecycle**:
  - Load `.wasm` files from disk (path defined in Config).
  - Compile `Module` once, instantiate `Instance` per request (or pool them).
  - Call exported function `on_request`.

### Control Plane (Go)

- **Configuration**:
  - Add `plugins` field to `Listener` or `Route`.
  - Proto definition for `Plugin` config (Path to wasm, arguments).

### Minimal MVP ABI (Application Binary Interface)

To avoid the complexity of full `proxy-wasm` for this iteration, we define a simple MVP ABI:

- **Guest (Wasm) Imports**:
  - `agw_log(level: i32, ptr: i32, len: i32)`
- **Guest Exports**:
  - `on_request() -> i32`: Returns 0 for Allow, 1 for Deny.

_(Future: Adopt full proxy-wasm standard)_

## 3. User Stories

- **US.4.1**: As a user, I want to block requests based on a specific header value using a Wasm plugin.
- **US.4.2**: I want to configure the Wasm plugin file path in `config.yaml`.

## 4. Functional Requirements

1.  **Config**: Support `plugins` list in YAML & Proto.
2.  **Loader**: Data Plane pre-loads Wasm modules on config update.
3.  **Runtime**: Secure execution with `wasmtime`.
4.  **Enforcement**: If Plugin returns Deny, AGW returns 403 Forbidden.

## 5. Success Criteria

- **Test**: A simple "Deny-All" Wasm plugin blocks traffic.
- **Test**: A "Log-Headers" Wasm plugin prints logs to stdout.
