# Feature 005: Custom CRD Support (GatewayRoute)

## 1. Goal

Implement a custom Kubernetes CRD (`GatewayRoute`) to allow users to configure AGW routing capabilities natively, including Plugin configuration, without relying on ambiguous Ingress annotations.

## 2. API Definition

**Group**: `agw.masallsome.io`
**Version**: `v1`
**Kind**: `GatewayRoute`

### Example Resource

```yaml
apiVersion: agw.masallsome.io/v1
kind: GatewayRoute
metadata:
  name: nginx-route
  namespace: default
spec:
  match: "/nginx"
  backend:
    service_name: "my-nginx"
    port: 80
  plugins:
    - name: "header-check"
      wasm_path: "plugins/header-check.wasm" # Or use a pre-registered name
      config:
        block_val: "curl"
```

## 3. Architecture

### Control Plane (Go)

- **Dynamic Client**: Use `client-go/dynamic` to watch `agw.masallsome.io/v1`.
- **Benefits**:
  - No need for deep glue code or code-generation for this iteration.
  - Flexible parsing of JSON/Unstructured data.
- **Logic**:
  - Watch `GatewayRoute`.
  - On Update: Parse `spec` -> Convert to `agwv1.Route`.
  - Push to Aggregator.

## 4. User Stories

- **US.5.1**: As a developer, I can define a `GatewayRoute` YAML that explicitly lists Wasm plugins.
- **US.5.2**: The Gateway automatically hot-reloads when I edit the CRD.

## 5. Implementation Steps

1.  **CRD Manifest**: Create `deploy/crd.yaml`.
2.  **Control Plane**:
    - Initialize `DynamicClient`.
    - Watch GVR `agw.masallsome.io/v1/gatewayroutes`.
    - Parse logic (Unstructured -> internal Route struct).
3.  **Verification**: Apply CRD, apply Route, Verify access.
