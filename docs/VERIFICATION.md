# AGW 验证指南

为了适应不同的开发阶段和测试需求，我们将验证分为三种场景。请根据您的目的选择合适的模式。

---

## 场景一：本地开发模式 (Local Development)

**🎯 验证目标**: 快速迭代业务逻辑、Wasm 插件开发、配置热更新。

### 操作步骤

1. **启动控制面 (Control Plane)**:

   ```bash
   cd control-plane
   # 确保 config.yaml 存在
   go run cmd/server/main.go
   ```

2. **启动数据面 (Data Plane)** (新终端):

   ```bash
   cd data-plane
   # 指定控制面地址
   export AGW_CONTROL_PLANE_URL="http://localhost:18000"
   export RUST_LOG=debug
   cargo run
   ```

3. **基础验证**:
   ```bash
   # 测试 HTTP 路由
   curl -v http://localhost:6188/new
   ```

---

## 场景二：Docker 环境验证 (Docker Environment)

**🎯 验证目标**: 在纯净的容器环境中验证全链路依赖 (Redis, DBs, Upstream) 和网络连通性。

### 1. 启动环境

我们在 `deploy/docker` 目录下准备了完整的一键启动环境。

```bash
cd deploy/docker
docker-compose up --build -d
```

### 2. 准备测试数据 (Data Seeding)

为了验证数据库插件，我们需要先在数据库中创建表并插入数据。

**Postgres (用于 users-pg)**:

```bash
# 进入 Postgres 容器执行 SQL
docker exec -it mas-postgres psql -U postgres -d mydb -c "CREATE TABLE IF NOT EXISTS users (id SERIAL PRIMARY KEY, username TEXT); INSERT INTO users (username) VALUES ('alice');"
```

**MySQL (用于 products-mysql)**:

```bash
# 进入 MySQL 容器执行 SQL
docker exec -it mas-mysql mysql -uroot -ppassword mydb -e "CREATE TABLE IF NOT EXISTS products (id INT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(255)); INSERT INTO products (name) VALUES ('apple');"
```

### 3. 执行验证

**Redis 限流测试**:

```bash
# 第一次: 200 OK
curl -v -H "X-User-ID: u1" http://localhost:6188/redis
# ... 连续执行 6 次 ...
# 第六次: 403 Forbidden (限流生效)
```

**Postgres 查询测试**:

```bash
# 默认查 Postgres
# 预期: Log 中打印 Query Result (如 ["alice"]), Curl 返回页面
curl -v -H "X-DB-Type: postgres" http://localhost:6188/db
```

**MySQL 查询测试**:

````bash
# 指定查 MySQL
# 预期: Log 中打印 Query Result (如 ["apple"]), Curl 返回页面
curl -v -H "X-DB-Type: mysql" http://localhost:6188/db

**Deny 插件测试**:

```bash
# 访问被禁止的路径
# 预期: HTTP 403 Forbidden
curl -v http://localhost:6188/new
````

**HTTPS 测试**:

```bash
# 预期: HTTPS 握手成功 (忽略证书错误) + 路由响应成功
curl -v -k https://localhost:6443/secure
```

````

---

## 场景三：Kubernetes 集群验证 (K8s Cluster)

**🎯 验证目标**: 验证 Operator、CRD、RBAC 权限及生产环境部署，以及插件对集群内服务（Redis/DB）的访问。

### 操作步骤

1. **构建并加载镜像** (以 Kind 为例):

   ```bash
   make docker-build
   kind load docker-image masapigateway/control-plane:latest masapigateway/data-plane:latest
   docker pull redis:alpine
   docker pull postgres:alpine
   docker pull mysql:8.0
   docker pull ealen/echo-server:latest

   kind load docker-image \
   redis:alpine \
   postgres:alpine \
   mysql:8.0 \
   ealen/echo-server:latest
````

2. **部署资源**:

   ```bash


   # 1. 启动依赖服务 (Redis, DBs, Upstream)
   kubectl apply -f deploy/kubernetes/k8s-deps.yaml
   kubectl apply -f deploy/kubernetes/upstream.yaml

   # 2. 准备测试数据 (Data Seeding)
   # ⚠️ 注意: 需等待 Redis/DB Pod 状态为 Running 后执行
   kubectl wait --for=condition=ready pod -l app=postgres --timeout=60s
   kubectl wait --for=condition=ready pod -l app=mysql --timeout=60s

   kubectl exec -it deployment/postgres -- psql -U postgres -d mydb -c "CREATE TABLE IF NOT EXISTS users (id SERIAL PRIMARY KEY, username TEXT); INSERT INTO users (username) VALUES ('bob_k8s');"
   kubectl exec -it deployment/mysql -- mysql -uroot -ppassword mydb -e "CREATE TABLE IF NOT EXISTS products (id INT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(255)); INSERT INTO products (name) VALUES ('banana_k8s');"

   # 3. 部署网关 (CRD, Deployment, RBAC)
   kubectl apply -f deploy/kubernetes/
   ```

3. **验证**:

   ```bash
   # 端口转发 Data Plane 服务到本地
   kubectl port-forward svc/mas-agw-data-plane 6188:80 &


   kubectl port-forward svc/mas-agw-data-plane 6443:443 &

   # 验证 Redis (动态路由)
   # 预期: Log (kubectl logs) 中打印 Redis 操作日志，Curl 收到 Echo Server 的 JSON
   curl -v -H "X-User-ID: k8s_user" http://localhost:6188/redis-crd

   # 验证 Postgres (动态路由)
   # 预期: Log 中打印 ["bob_k8s"]，Curl 收到 Echo Server 的 JSON
   curl -v -H "X-DB-Type: postgres" http://localhost:6188/db-crd

   # 验证 MySQL (动态路由)
   # 预期: Log 中打印 ["banana_k8s"]，Curl 收到 Echo Server 的 JSON
   curl -v -H "X-DB-Type: mysql" http://localhost:6188/db-crd

   # 验证 Deny 插件 (动态路由)
   # 预期: HTTP 403 Forbidden
   curl -v http://localhost:6188/deny-crd

   # 验证 HTTPS (需端口转发 6443)
   kubectl port-forward svc/mas-agw-data-plane 6443:443 &
   # 预期: HTTPS 握手成功 (忽略证书错误) + 路由响应成功
   curl -v -k https://localhost:6443/redis-crd
   ```

---

## 目录结构说明

- **deploy/kubernetes/**: 包含所有 K8s 部署文件。
  - `k8s-deps.yaml`: Redis/DB 依赖服务。
  - `configmap.yaml`: 网关核心配置。
  - `crd.yaml`, `deployment.yaml` 等: 网关组件。
- **deploy/docker/**: 包含 Docker Compose 环境配置。
- **plugins/**: 包含所有演示用的 Wasm 插件源码。
- **target/**: 编译产物目录。
