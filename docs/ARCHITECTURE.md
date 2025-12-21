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

### 3. 协议定义 (`proto/`)

- 定义了 `Listener`, `Route`, `Cluster`, `TlsConfig` 等 Protobuf 消息。
- 作为 CP 和 DP 之间 gRPC 通信的标准接口。

## 数据流转 (Data Flow)

1. **配置变更**: 管理员更新 `GatewayRoute` CRD 或 K8s Service 发生变化。
2. **检测**: 控制面的 Controller 通过 Informer 机制检测到变更。
3. **处理**: Controller 更新内存中的注册表 (`Registry`)。
4. **广播**: `AgwServer` 将 Registry 中的数据合并生成新的 `ConfigSnapshot`。
5. **推送**: 控制面通过 gRPC 流将快照推送给所有已连接的数据面。
6. **应用**: 数据面接收快照，原子更新配置引用。新进入的请求将立即使用新配置。TLS 监听器也会相应更新（当前 MVP 版本可能涉及端口重新绑定）。

## 扩展性

- **CRDs**: 可以通过扩展 `GatewayRoute` 定义更复杂的路由能力。
- **Wasm**: 用户可以使用 Rust/Go/AssemblyScript 编写插件，编译为 `.wasm` 文件，并在路由配置中引用，实现自定义业务逻辑。
