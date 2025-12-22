# AGW (API Gateway)

基于 **Rust (Pingora)** 和 **Go** (Control Plane) 构建的现代高性能 API 网关。

## 功能特性

- **动态配置 (xDS)**: 支持监听器、路由和集群配置的热更新，无停机时间。
- **Kubernetes 原生**: 通过 Watch API 自动发现 K8s Services 和 Endpoints。
- **自定义 CRD 支持**: 使用 `GatewayRoute` CRD 定义高级路由规则。
- **TLS 终结 (HTTPS)**: 支持从 Kubernetes Secrets 动态加载 TLS 证书。
- **Wasm 插件**: 集成 Wasmtime，支持在请求路径中执行自定义逻辑（如鉴权、流控）。

## 架构设计

详见 [架构设计文档](docs/ARCHITECTURE.md)。

## 快速开始

详见 [验证指南](docs/VERIFICATION.md)。

### 本地快速运行 (Local)

需要两个终端窗口：

1. **Control Plane**: `cd control-plane && go run cmd/server/main.go`
2. **Data Plane**: `cd data-plane && AGW_CONTROL_PLANE_URL=http://localhost:18000 cargo run`

然后访问: `curl http://localhost:6188/new`

## 项目结构

- `control-plane/`: Go 语言编写的控制面 (xDS Server, K8s Controllers)。
- `data-plane/`: Rust 语言编写的数据面 (基于 Pingora Proxy)。
- `proto/`: gRPC 接口定义。
- `plugins/`: Wasm 插件源码。
- `deploy/`: Kubernetes 部署清单 (CRDs)。
