# 验证指南

本指南介绍了如何验证 AGW 的核心功能。

## 前置条件

- **Go 1.22+**
- **Rust (Cargo)**
- **Docker** (强烈推荐用于验证 TLS 功能，以避免 macOS 本地的 OpenSSL 兼容性问题)
- **Kubectl** (可选，也可以通过模拟文件进行测试)

## 1. 环境搭建

### 启动控制面 (Control Plane)

```bash
cd control-plane
go run cmd/server/main.go
# 监听端口: 18000 (gRPC)
```

### 启动数据面 (Data Plane) - 本地模式

```bash
cd data-plane
# 可选: 设置日志级别
export RUST_LOG=debug
# 可选: 设置控制面地址
export AGW_CONTROL_PLANE_URL="http://localhost:18000"

cargo run
# 监听端口: 取决于配置 (默认 6188 HTTP, 6443 HTTPS)
```

### 启动数据面 (Data Plane) - Docker 模式

使用 Docker 验证可以确保 TLS 功能在标准的 Linux/OpenSSL 环境下运行。

1. **构建镜像**:

   ```bash
   # 请在项目根目录下执行
   docker build -f data-plane/Dockerfile -t agw-data-plane .
   ```

2. **运行容器**:
   ```bash
   # 假设此时控制面运行在宿主机的 18000 端口
   # 使用 host.docker.internal 访问宿主机
   docker run -p 6188:6188 -p 6443:6443 \
     -e AGW_CONTROL_PLANE_URL="http://host.docker.internal:18000" \
     agw-data-plane
   ```

## 2. 功能验证

### HTTP 路由 (Feature A)

1. 确保 `config.yaml` 中包含路由配置 (例如: `/new` -> `my-local-cluster`)。
2. 发起请求: `curl -v http://localhost:6188/new`
3. 观察响应 (或 Wasm 插件的拦截日志)。

### TLS 终结 / HTTPS (Feature B)

1. **生成自签名证书**:
   ```bash
   openssl req -x509 -newkey rsa:2048 -nodes -keyout server.key -out server.crt -days 365 -subj "/CN=localhost"
   ```
2. **创建 Secret** (模拟或 K8s):
   ```bash
   kubectl create secret tls my-tls-secret --cert=server.crt --key=server.key
   # 如果没有 K8s 集群，控制面需要配置为从本地加载或模拟 Registry 数据。
   ```
3. **更新配置**:
   确认 `control-plane/config.yaml` 中配置了 6443 端口的监听器，并引用了 `tls: { secret_name: "my-tls-secret" }`。
4. **测试 HTTPS**:
   ```bash
   curl -k -v https://localhost:6443/secure
   ```
   - **成功**: TLS 握手成功 (返回 HTTP 200/404/502)。
   - **失败**: Connection Refused (端口未开放) 或 Reset (握手协议不兼容)。

### Wasm 插件

1. 编译插件: `cd plugins/deny-all && cargo build --target wasm32-unknown-unknown --release`
2. 在路游配置中启用该插件。
3. 验证请求是否被拦截 (HTTP 403 Forbidden)。
