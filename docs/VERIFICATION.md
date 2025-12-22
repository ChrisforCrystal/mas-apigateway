# AGW 验证指南

为了适应不同的开发阶段和测试需求，我们将验证分为三种场景。请根据您的目的选择合适的模式。

---

## 场景一：本地开发模式 (Local Development)

**🎯 验证目标**:

- **基础业务逻辑**: 路由转发、请求头处理。
- **Wasm 插件**: 验证插件能否正确加载和拦截请求。
- **热更新**: 修改 `config.yaml` 或 Wasm 文件，验证无需重启即可生效。

**✅ 适用场景**: 日常编码、快速调试 (Debug)、功能开发。
**⚠️ 局限性**: 无法验证 TLS (因 macOS/Linux OpenSSL 差异)，K8s 交互仅限于读取 kubeconfig。

### 操作步骤

1. **启动控制面 (Control Plane)**:
   > ⚠️ **注意**: 本地运行时如果缺少 K8s 连接，HTTPS 监听器因缺少证书将无法启动，但这不影响 HTTP (6188) 功能验证。
   ```bash
   cd control-plane
   # 确保 config.yaml 存在
   go run cmd/server/main.go
   ```
2. **启动数据面 (Data Plane)**:
   > 数据面会尝试连接控制面获取动态配置。
   ```bash
   cd data-plane
   # 指定控制面地址
   export AGW_CONTROL_PLANE_URL="http://localhost:18000"
   # 开启详细日志
   export RUST_LOG=debug
   cargo run
   ```
3. **测试**:
   - HTTP 请求: `curl -v http://localhost:6188/new`
   - **Wasm 插件配置与验证**:
     1. **编译插件**:
        ```bash
        cd plugins/deny-all
        cargo build --target wasm32-unknown-unknown --release
        ```
     2. **修改配置** (`control-plane/config.yaml`):
        在路由下添加 `plugins` 字段 (请使用绝对路径):
        ```yaml
        routes:
          - match: "/new"
            cluster: "my-local-cluster"
            plugins:
              - name: "deny-curl"
                wasm_path: "/Create/Absolute/Path/To/plugins/deny-all/target/wasm32-unknown-unknown/release/deny_all.wasm"
        ```
     3. **验证拦截**:
        - `curl -v http://localhost:6188/new` -> **403 Forbidden** (因为 User-Agent 包含 curl)
        - `curl -v -H "User-Agent: browser" http://localhost:6188/new` -> **200 OK**

---

## 场景二：Docker 环境验证 (Docker Environment)

**🎯 验证目标**:

- **TLS 终结 (HTTPS)**: 验证在标准 Linux/OpenSSL 环境下证书加载和握手是否正常。
- **环境一致性**: 验证构建产物 (`Dockerfile`) 可在 Linux 容器中正常运行。

**✅ 适用场景**: 提交代码前验证、解决跨平台库兼容性问题 (如 TLS 报错)。

### 操作步骤

1. **构建镜像**:
   ```bash
   make docker-build
   # 或者: docker build -f data-plane/Dockerfile -t masapigateway/data-plane:latest .
   ```
2. **运行数据面容器**:
   ```bash
   # 假设控制面仍在本地运行 (端口 18000)
   docker run --rm -p 6188:6188 -p 6443:6443 \
     -e AGW_CONTROL_PLANE_URL="http://host.docker.internal:18000" \
     masapigateway/data-plane:latest
   ```
3. **测试 HTTPS**:
   ```bash
   curl -k -v https://localhost:6443/secure
   ```
   _在此模式下，TLS 握手应成功。_

---

## 场景三：集群集成验证 (K8s Cluster)

**🎯 验证目标**:

- **Operator 模式**: 验证控制面能否正确 watch K8s 资源 (Services, Secrets, CRDs)。
- **RBAC 权限**: 验证 ServiceAccount 是否有权限读取资源。
- **CRD 动态路由**: 验证 `GatewayRoute` 自定义资源的生效情况。
- **全链路部署**: 验证 Deployment/Service/ConfigMap 的定义是否正确。

**✅ 适用场景**: 集成测试、生产部署前验收、验证 K8s 特有功能。

### 操作步骤

1. **构建镜像**:
   ```bash
   make docker-build
   # 构建 Control Plane 和 Data Plane 镜像
   # 如果使用 Kind，还需要加载镜像: kind load docker-image masapigateway/control-plane:latest masapigateway/data-plane:latest
   ```
2. **部署 Operator**:
   ```bash
   make deploy
   # 这将自动应用 RBAC, CRD, Deployment 到当前 K8s 集群
   ```
3. **创建测试资源**:
   ```bash
   # 1. 创建 TLS Secret
   kubectl create secret tls my-tls-secret --cert=server.crt --key=server.key
   # 2. 创建动态路由 (CRD)
   kubectl apply -f k8s-test-crd.yaml
   ```
4. **验证**:

   - **查看日志**: `kubectl logs -l app=mas-agw-control-plane` 确认监听到事件。
   - **访问服务**:

     ```bash

     kubectl port-forward svc/mas-agw-data-plane 6188:80
     curl -k -v https://localhost:6443/dynamic
     # 端口转发到本地进行测试
     kubectl port-forward svc/mas-agw-data-plane 6443:443
     curl -k -v https://localhost:6443/dynamic
     ```

---

## 总结

| 验证模式     | 关注点              | 核心优势               |
| :----------- | :------------------ | :--------------------- |
| **本地开发** | 业务逻辑、Wasm      | 开发速度快，Debug 方便 |
| **Docker**   | TLS、二进制兼容性   | 环境纯净，消除系统差异 |
| **K8s 集群** | Operator、CRD、RBAC | 真实场景，集成测试     |
