# AGW (API Gateway)

基于 **Rust (Pingora)** 和 **Go** (Control Plane) 构建的现代高性能 API 网关。

## 功能特性

- **动态配置 (xDS)**: 支持监听器、路由和集群配置的热更新，无停机时间。
- **Kubernetes 原生**: 通过 Watch API 自动发现 K8s Services 和 Endpoints。
- **自定义 CRD 支持**: 使用 `GatewayRoute` CRD 定义高级路由规则。
- **TLS 终结 (HTTPS)**: 支持从 Kubernetes Secrets 动态加载 TLS 证书。
- **Wasm 组件模型 (WIT)**: 采用最新的 Wasm Component Model 标准，插件编写更安全、接口更丰富（支持 Redis/HTTP 本地调用）。
- **eBPF 流量加速**: 集成 eBPF (SockMap) 技术，实现 Sidecar 与应用容器间的流量短路 (Kernel Bypass)，大幅降低延迟。

## 架构设计

详见 [架构设计文档](docs/ARCHITECTURE.md)。

## 快速开始

详见 [验证指南](docs/VERIFICATION.md)。

### Docker 环境快速验证 (推荐)

本项目提供了完整的 Docker Compose 环境，包含 Control Plane, Data Plane (Sidecar), eBPF Agent 以及测试用的 Redis/MySQL/Upstream。

```bash
cd deploy/docker
docker-compose up -d --build
```

验证服务是否正常运行：

```bash
# 验证 Wasm 插件 (WIT)
curl -v http://localhost:6188/redis-crd

# 查看 eBPF Agent 日志
docker logs -f mas-ebpf-agent
```

### 本地开发运行 (Local)

需要三个终端窗口：

1. **Control Plane**: `cd control-plane && go run cmd/server/main.go`
2. **Data Plane**: `cd data-plane && AGW_CONTROL_PLANE_URL=http://localhost:18000 cargo run`
3. **eBPF Agent**: (需要 Linux 环境及 Root 权限，Mac 上无法运行)

然后访问: `curl http://localhost:6188/new`

## 项目结构

- `control-plane/`: Go 语言编写的控制面 (xDS Server, K8s Controllers)。
- `data-plane/`: Rust 语言编写的数据面 (基于 Pingora Proxy, 集成 Wasmtime)。
- `ebpf-agent/`: Rust (Aya) 编写的 eBPF 代理，负责内核级流量加速。
- `proto/`: gRPC 接口定义。
- `plugins/`: Wasm 插件源码 (基于 WIT 接口)。
- `deploy/`: Kubernetes 部署清单 (CRDs) 及 Docker Compose 环境。
