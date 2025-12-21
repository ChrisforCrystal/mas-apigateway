# Project Roadmap: AGW

**Current Phase**: Foundation Built (001)

| Feature ID             | Title                           | Description                                                                                                                | Status      | Dependencies |
| :--------------------- | :------------------------------ | :------------------------------------------------------------------------------------------------------------------------- | :---------- | :----------- |
| **001-agw-core**       | **Core Initialization**         | Project scaffolding, CP/DP separation, gRPC connection.                                                                    | ✅ **Done** | None         |
| **002-dynamic-config** | **Dynamic Configuration (xDS)** | Define `Listener`, `Route`, `Cluster` data models. Implement dynamic config hot-reloading in Pingora and xDS server in Go. | ⏹️ **Next** | 001          |
| **003-k8s-discovery**  | **K8s Service Discovery**       | Control Plane watches K8s Services/Endpoints and updates `Cluster` configs dynamically.                                    | ⏹️ Pending  | 002          |
| **004-wasm-runtime**   | **Wasm Plugin Support**         | Integrate Wasmtime into Data Plane to allow executing custom logic in request path.                                        | ⏹️ Pending  | 001          |
| **005-tls-manager**    | **TLS Termination**             | Support HTTPS listeners and dynamic certificate loading (from K8s Secrets or local).                                       | ⏹️ Pending  | 002          |

## Dependency Graph

```maid
graph TD
    A[001-agw-core] --> B[002-dynamic-config]
    A --> D[004-wasm-runtime]
    B --> C[003-k8s-discovery]
    B --> E[005-tls-manager]
```
