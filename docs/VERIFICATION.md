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

---

## 补充：证书与安全概念 (Certificate Management)

### 1. 核心文件说明

在 TLS/HTTPS 配置中，通常涉及以下三种文件：

| 文件后缀        | 全称                   | 说明                                                           | 谁持有                        |
| :-------------- | :--------------------- | :------------------------------------------------------------- | :---------------------------- |
| **.key**        | Private Key (私钥)     | **核心机密**。用于解密数据和数字签名。泄露意味着安全防线崩塌。 | **仅网关服务端** (Data Plane) |
| **.crt / .pem** | Certificate (公钥证书) | 包含公钥和身份信息。相当于“身份证”，用于向对方证明身份。       | **服务端持有，客户端验证**    |
| **CA**          | Certificate Authority  | 签发证书的权威机构。CA 的根证书用于验证其他证书的合法性。      | **客户端** (放入信任库)       |

### 2. 本项目证书流转

`masapigateway` 采用 **Server-Side TLS (单向认证)** 模式：

1.  **生成 (Generate)**: 管理员使用 `openssl` 生成 `server.key` (私钥) 和 `server.crt` (自签证书)。
2.  **上传 (Upload)**: 通过 `kubectl create secret generic` 将其存入 Kubernetes Secret。
3.  **分发 (Distribute)**: Control Plane 读取 Secret 并通过 gRPC 全量推送给 Data Plane。
4.  **使用 (Usage)**: Data Plane 启动 HTTPS 监听。
5.  **验证 (Verify)**: Client (curl) 发起请求，网关出示 `server.crt`。

### 3. 如何生成测试证书

我们在 `Makefile` 中并未集成生成逻辑，你需要手动生成：

```bash
# 1. 生成私钥
openssl genrsa -out server.key 2048

# 2. 生成自签证书 (有效期 365 天)
# -nodes: 不加密私钥
# -subj: 避免交互式输入信息，CN (Common Name) 必须匹配域名 (这里是 localhost)
openssl req -new -x509 -sha256 -key server.key -out server.crt -days 365 -nodes \
  -subj "/C=CN/ST=Beijing/L=Beijing/O=MasAllSome/OU=Gateway/CN=localhost"
```

### 4. 常见问题

- **为什么 curl 需要 `-k`?**
  因为我们使用自签证书，curl 默认不信任非权威 CA 签发的证书。`-k` 意为 "Insecure"，即跳过验证。
- **如何不加 `-k` 也能访问？**
  客户端需要显式信任你的证书：
  ```bash
  curl --cacert server.crt https://localhost:6443/
  ```

### 5. 深入理解 CA 与信任链

针对 "CA 在通信中起什么作用" 这一常见疑问：

- **CA 的角色**: 类似于现实生活中的 **“公证处”** 或 **“身份证签发机关”**。
  - 它**不参与**你（客户端）和网关（服务端）的日常加密通信（数据不经过 CA）。
  - 它的核心作用是 **“信用背书”**：在握手阶段，证明网关出示的证书是合法的。
- **信任的传递逻辑**:
  1.  **客户端信任 CA**: 你手动将 CA 的根证书（Root Cert）加入到操作系统或浏览器的 **“受信任的根证书颁发机构”** 列表中。
  2.  **CA 信任网关**: CA 用自己的私钥给网关的证书（server.crt）进行了**数字签名**。
  3.  **结果**: 当你访问网关时，浏览器看到网关证书上有 CA 的签名，因为你信任 CA，所以你也自动信任了这个网关。
- **关于“带着公钥访问”**:
  - 客户端**不需要**发送公钥给网关。
  - 客户端使用的是**本地信任库中 CA 的公钥**，去解密和验证网关证书上的签名。一旦验证通过，就建立连接。

### 6. mTLS (双向认证) 科普

既然提到了 mTLS，这里也简单介绍一下它的区别：

- **场景**: 只有在 **极高安全要求**（如银行 U 盾、微服务网格 Istio）场景下才会开启。
- **逻辑 (双向)**:
  1.  **Client -> Gateway**: 客户端验证网关证书（和上面一样）。
  2.  **Gateway -> Client**: **新增步骤**。网关要求客户端：“出示你的证件”。
  3.  **Client Response**: 客户端发送自己的证书（Client Cert）和签名。
  4.  **Gateway Verify**: 网关拿着 **“Client CA”** (必须预先配置在网关上) 去验证客户端证书是真的。
      - _(回答你的疑问)_: 是的，这意味着 **Client 必须持有由网关信任的 CA 所签发的证书**。换句话说，确实是“网关（或者其背后的组织）之前发给 Client 的”。
- **差异点**:
  - 单向认证：只有网关有 `.key`。
  - 双向认证：**双方都有** 自己的 `.key` 和 `.crt`，且**双方都有** 对方的 CA 公钥来进行验证。
