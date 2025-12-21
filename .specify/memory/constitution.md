<!--
SYNC IMPACT REPORT
==================
Version: 1.0.0 (Initial Ratification)
Changes:
- Established Core Principles: Architecture, Safety, Performance.
- Defined Technology Stack Constraints.
- Defined Directory Structure.
- Added Governance section.
Templates Status:
- .specify/templates/plan-template.md: ✅ Compatible
- .specify/templates/spec-template.md: ✅ Compatible
- .specify/templates/tasks-template.md: ✅ Compatible
Follow-up:
- None
-->

# AGW (AI Gateway) 工程宪法

## 1. 愿景与核心原则 (Vision & Core Principles)

**项目代号**: AGW
**定位**: 工业级、存算分离的云原生 AI 网关。
**核心理念**: **Ambient Spec-Driven Development**。所有的代码变更必须先由 Spec 定义，经 Plan 确认，最后由 AI 执行。禁止无文档的“意念编程”(Vibe Coding)。

### 三大铁律 (The Iron Laws)

1.  **架构铁律**: 严格遵循 **Control Plane (Go)** 与 **Data Plane (Rust)** 分离。两者通过 gRPC Streaming 通信，严禁共享数据库或内存状态。
2.  **安全铁律**: 数据面严禁包含硬编码的业务逻辑。所有动态逻辑（鉴权、计费、清洗）必须封装为 **Wasm (WebAssembly)** 插件，运行在 Wasmtime 沙箱中。
3.  **性能铁律**: 数据面必须利用 Rust 的所有权机制和 Pingora 的零拷贝特性，确保极低的各种 Overhead。

---

## 2. 技术栈约束 (Technology Constraints)

### A. 数据面 (The Muscle) - `data-plane/`

- **语言**: Rust (2021 Edition)。
- **核心引擎**: **Cloudflare Pingora**。这是绝对核心，不得替换为 Axum/Hyper/Actix。
- **插件运行时**: `wasmtime`。
- **配置管理**: `arc-swap` (实现无锁热加载)。
- **通信协议**: `tonic` (gRPC Client)，监听控制面的 xDS 流。

### B. 控制面 (The Brain) - `control-plane/`

- **语言**: Go (1.22+)。
- **RPC 框架**: `google.golang.org/grpc`。
- **K8s 集成**: `client-go` & `controller-runtime`。
- **职责**: 监听 K8s CRD 变化 -> 转换为通用配置 -> 通过 gRPC 推送给 Rust 节点。

### C. 协议层 (The Glue) - `proto/`

- **工具**: `protoc` (Proto3)。
- **定义**: 必须包含类似 xDS 的结构 (RDS/CDS/EDS)，但需简化为 AGW 专用版本。

### D. 命令行与工具 (The Interface) - `cli/`

- **语言**: Rust。
- **框架**: `clap`。
- **职责**: 项目脚手架、代码生成 (Codegen)、以及作为 Spec Kit 的执行入口。

---

## 3. 目录结构规范 (Directory Structure)

项目必须保持以下 Monorepo 结构，AI 在生成文件时不得偏离：

```text
agw/
├── .specify/            # [Spec Kit] 存放 spec.md, history.md
├── constitution.md      # [Constitution] 本文件
├── cli/                 # [Rust] 开发者 CLI 工具
├── proto/               # [Protobuf] gRPC 定义
├── control-plane/       # [Go] 控制面源码
├── data-plane/          # [Rust] Pingora 代理源码
├── plugins/             # [Rust/Go] Wasm 插件源码
└── deploy/              # [K8s] Helm Charts
```

---

## 4. 治理 (Governance)

### 版本与生效 (Version & Ratification)

- **当前版本**: 1.0.0
- **生效日期**: 2025-12-21
- **最后修订**: 2025-12-21

### 修订程序 (Amendment Procedure)

1. **版本控制**: 宪法遵循语义化版本控制 (Semantic Versioning)。

   - **MAJOR**: 原则性变更、移除铁律或技术栈重大替换。
   - **MINOR**: 新增原则、栏目或对现有原则的非破坏性扩展。
   - **PATCH**: 措辞修正、格式调整或澄清说明。

2. **合规审查**:
   - 所有的 Implementation Plan 必须包含 `Constitution Check` 章节。
   - 若 Plan 违反宪法原则，必须在 Spec 阶段发起宪法修正案 (Amendment Request)，版本升级后方可执行。
   - 严禁在未修订宪法的情况下通过“特例”绕过铁律。
