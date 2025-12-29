# 架构设计文档

AGW 采用经典的 控制面 (Control Plane) / 数据面 (Data Plane) 分离架构，设计灵感来源于 Envoy/Istio，基于 Pingora 框架构建数据面以获得高吞吐和低延迟特性。

## 核心组件

### 1. 控制面 (Control Plane) - `control-plane/`

- **语言**: Go
- **角色**: 配置的"真理之源" (Source of Truth)。
- **模块**:
  - **Watcher**: 监听本地 `config.yaml` 文件的静态配置变更。
  - **K8s Controller**: 监听 K8s 的 `Services`, `EndpointSlices` 以及自定义的 `GatewayRoute` CRD。
  - **Secret Controller**: 监听 K8s 的 `Secrets` (TLS 类型) 并将其缓存在内存中。
  - **xDS Server (gRPC)**: 负责将静态和动态配置合并为 `ConfigSnapshot`，并通过 gRPC 流实时推送到所有连接的数据面节点。

### 2. 数据面 (Data Plane) - `data-plane/`

- **语言**: Rust (Pingora 框架)
- **角色**: 流量代理与策略执行点。
- **模块**:
  - **xDS Client**: 连接控制面，接收配置快照更新。
  - **Dynamic Proxy**: 实现了 `ProxyHttp` trait。使用 `ArcSwap` 技术实现配置的热替换 (Hot-Swap)。
  - **Wasm Runtime**: 内置 Wasmtime 运行时，用于执行路由中定义的插件逻辑 (如鉴权、限流)。
  - **TLS Manager**: 根据配置动态初始化 TLS 监听器，支持从内存加载证书。

### 3. eBPF 代理 (eBPF Agent) - `ebpf-agent/`

- **语言**: Rust (Aya 框架) + eBPF (Kernel C/Rust)
- **角色**: Sidecar 流量加速器。
- **机制**:
  - **SockMap**: 维护同节点内 Socket 的映射表。
  - **Traffic Bypass**: 利用 `BPF_MAP_TYPE_SOCKHASH` 和 `BPF_PROG_TYPE_SK_MSG`，拦截本地 Socket 的 `sendmsg` 系统调用。
  - **Short-Circuit**: 如果发现目标 Socket 也在同节点（例如 Data Plane -> App），直接将数据重定向到目标 Socket 的接收队列 (Ingress Queue)，绕过整个 TCP/IP 协议栈（无 IP 路由、无 iptables 处理），大幅降低延迟并提升吞吐量。
- **部署**: 作为 DaemonSet 或特权 Sidecar 运行，需要 `CAP_SYS_ADMIN` / `CAP_BPF` 权限。

### 4. 协议定义 (`proto/`)

- 定义了 `Listener`, `Route`, `Cluster`, `TlsConfig` 等 Protobuf 消息。
- 作为 CP 和 DP 之间 gRPC 通信的标准接口。

## 关键技术特性

### Wasm 组件模型 (Component Model)

Data Plane 采用最新的 **Wasm Component Model (WIT)** 标准集成 Wasmtime。
相比传统的 Wasm Module，Component Model 提供了：

- **强类型接口**: 通过 `.wit` 文件定义 Host (网关) 与 Guest (插件) 之间的接口契约。
- **结构化通信**: 支持复杂数据类型（如 Struct, Resource, Result）的高效传递，由 `wit-bindgen` 自动生成绑定代码，无需手动处理内存指针。
- **细粒度能力**: 插件只能访问在 WIT 中明确导出的 Host Function（如 `agw:redis`, `agw:http-request`），安全性更高。

## 数据流转 (Data Flow)

1. **配置变更**: 管理员更新 `GatewayRoute` CRD 或 K8s Service 发生变化。
2. **检测**: 控制面的 Controller 通过 Informer 机制检测到变更。
3. **处理**: Controller 更新内存中的注册表 (`Registry`)。
4. **广播**: `AgwServer` 将 Registry 中的数据合并生成新的 `ConfigSnapshot`。
5. **推送**: 控制面通过 gRPC 流将快照推送给所有已连接的数据面。
6. **应用**: 数据面接收快照，原子更新配置引用。新进入的请求将立即使用新配置。
7. **加速 (eBPF)**: 当 Data Plane 转发流量给同 Pod/Node 的 Upstream 时，eBPF Agent 会自动短路 TCP 堆栈，实现零拷贝级别的转发。

## 扩展性

- **CRDs**: 可以通过扩展 `GatewayRoute` 定义更复杂的路由能力。
- **Wasm**: 用户可以使用 Rust (编译为 `wasm32-wasi`) 编写插件，通过 `wit-bindgen` 调用网关提供的 Redis、Log 等能力，灵活定制业务逻辑。
