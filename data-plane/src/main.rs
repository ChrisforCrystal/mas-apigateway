use arc_swap::ArcSwap;
use async_trait::async_trait;
use pingora::proxy::ProxyHttp;
use pingora::proxy::Session;
use pingora::proxy::http_proxy_service;
use pingora::server::Server;
use pingora::server::configuration::Opt;
use std::sync::Arc;

mod client;
use client::AgwClient;
mod wasm;
use wasm::WasmRuntime;
// We need to import the proto types. They are re-exported in client usually or accessible.
// client.rs exposes Node. We need ConfigSnapshot too.
// Let's rely on client code to return us something or expose it.
// client.rs: pub mod agw { ... }
// We can use client::agw::v1::ConfigSnapshot;

pub struct AgwProxy {
    // 【配置存储核心】 Arc<ArcSwap<T>>
    // 这是一个非常经典的 "Read-Copy-Update" (RCU) 模式，专为读多写少的场景设计。
    //
    // 1. Arc<...>: 使得多线程可以共享同一个 "配置指针"。Pingora 的每个 worker 线程都持有这个 Arc。
    // 2. ArcSwap<...>: 实现了【无锁替换】(Lock-Free Swap)。
    //    - 读 (Read): 当成千上万个请求进来时，它们通过 `load()` 拿到当前的配置快照。这个操作极快，不需要抢锁 (Mutex)。
    //    - 写 (Write): 当配置更新时，后台线程通过 `store()` 将旧配置原子替换为新配置。
    //    - 效果: 更新配置的一瞬间，正在处理的旧请求继续用旧配置跑完，新进来的请求立刻用新配置。
    config: Arc<ArcSwap<client::agw::v1::ConfigSnapshot>>,
    wasm: WasmRuntime,
}

#[async_trait]
impl ProxyHttp for AgwProxy {
    type CTX = ();
    fn new_ctx(&self) -> Self::CTX {
        ()
    }

    // 【阶段 1: 请求过滤器 (Request Filter)】
    // 这是请求处理的第一道关卡。Pingora 会在接收到请求头后立即调用此函数。
    // 在这里，我们可以：
    // 1. 读取全局配置 (ArcSwap)
    // 2. 匹配路由 (Routing)
    // 3. 执行 Wasm 插件 (鉴权、限流等)
    // 4. 决定请求是继续转发 (return false) 还是直接拦截响应 (return true)
    async fn request_filter(
        &self,
        session: &mut Session,
        _ctx: &mut Self::CTX,
    ) -> pingora::Result<bool> {
        // 1. 获取最新配置 (RCU - 用于读)
        // load() 返回一个临时的 Guard，保证我们在使用期间配置不会被释放
        let config = self.config.load();
        let path = session.req_header().uri.path();
        let _host = session.req_header().uri.host().unwrap_or("");

        // 2. 匹配路由 (Routing)
        // MVP: 简单遍历路由表 (生产环境通常使用线段树、radix tree 或者 hash map)
        for route in &config.routes {
            // 前缀匹配 (Prefix Match)
            if path.starts_with(&route.path_prefix) {
                // 3. 执行插件链 (Wasm Plugins)
                if !route.plugins.is_empty() {
                    // 准备工作：把 Pingora 的 Header 转换成 Wasm 能懂的 HashMap
                    let mut headers = std::collections::HashMap::new();
                    for (name, value) in session.req_header().headers.iter() {
                        if let Ok(v_str) = value.to_str() {
                            headers.insert(name.to_string(), v_str.to_string());
                        }
                    }

                    // 遍历执行该路由下的所有插件
                    for plugin in &route.plugins {
                        // 调用 Wasm 运行时的 run_plugin
                        // 注意：这里 clone 了一份 headers 传给 Wasm
                        match self.wasm.run_plugin(&plugin.wasm_path, headers.clone()) {
                            Ok(allow) => {
                                if !allow {
                                    // 插件拒绝 (如 Wasm 返回 1)
                                    // 直接响应 403 Forbidden
                                    let _ = session.respond_error(403).await;
                                    return Ok(true); // True = 请求已处理，不再转发给 upstream_peer
                                }
                            }
                            Err(e) => {
                                // 插件执行出错 (如 Wasm 崩溃)
                                // 安全起见返回 500
                                eprintln!("Wasm Plugin Error [{}]: {}", plugin.name, e);
                                let _ = session.respond_error(500).await;
                                return Ok(true);
                            }
                        }
                    }
                }
                // 路由匹配成功 & 插件全通过 -> 进入下一阶段
                // 返回 false 告诉 Pingora: "我没处理完，请继续交给 upstream_peer 处理"
                return Ok(false); 
            }
        }

        // 4. 没有匹配到任何路由 -> 404 Not Found
        // 手动发送 404 响应
        let _ = session.respond_error(404).await;
        Ok(true) // 请求结束
    }

    // 【阶段 2: 上游节点选择 (Upstream Peer Selection)】
    // 如果 request_filter 返回 Ok(false)，Pingora 就会调用这个函数。
    // 我们的任务是：决定把请求转发给哪个后端 IP:PORT。
    async fn upstream_peer(
        &self,
        session: &mut Session,
        _ctx: &mut Self::CTX,
    ) -> pingora::Result<Box<pingora::upstreams::peer::HttpPeer>> {
        let config = self.config.load();
        let path = session.req_header().uri.path();

        // 1. 重新匹配路由 (Route Lookup)
        // TODO: 这里目前有些低效，因为在 request_filter 里已经匹配过一次了。
        // 理想做法是在 request_filter 里把匹配到的 Cluster Name 存到 CTX 里传递过来。
        let mut cluster_name = "";
        for route in &config.routes {
            if path.starts_with(&route.path_prefix) {
                cluster_name = &route.cluster_id;
                break;
            }
        }

        if cluster_name.is_empty() {
            // 理论上不会发生，因为 request_filter 已经拦截了无效路由
            // 防御性编程：返回 502 Bad Gateway
            return Err(pingora::Error::create(
                pingora::ErrorType::HTTPStatus(502),
                pingora::ErrorSource::Upstream,
                Some("no route match".into()),
                None,
            ));
        }

        // 2. 服务发现 (Service Discovery)
        // 根据 cluster_name 在配置中找到对应的 Cluster 定义
        let cluster = config.clusters.iter().find(|c| c.name == cluster_name);
        if let Some(c) = cluster {
            // 3. 负载均衡 (Load Balancing)
            // MVP: 简单地选择第一个 Endpoint (First Available)
            // 生产环境应在此实现 RoundRobin / Random / LeastReq 等算法，并结合健康检查。
            if let Some(endpoint) = c.endpoints.first() {
                let addr = (endpoint.address.as_str(), endpoint.port as u16);
                
                // 4. 构造 Upstream Peer
                // 告诉 Pingora 转发的目标地址
                let peer = Box::new(pingora::upstreams::peer::HttpPeer::new(
                    addr,           // 目标 IP:PORT (如 10.244.1.5:8080)
                    false,          // TLS: 是否使用 HTTPS 连接上游 (这里 MVP 暂不支持 upstream TLS)
                    "".to_string(), // SNI: 如果是 HTTPS，这里填域名
                ));
                return Ok(peer);
            }
        }
        
        // 找到了 Cluster 但没有可用 Endpoint (可能 Pod 还没 Ready)
        // 返回 503 Service Unavailable
        Err(pingora::Error::create(
            pingora::ErrorType::HTTPStatus(503),
            pingora::ErrorSource::Upstream,
            Some("no healthy upstream".into()),
            None,
        ))
    }
}

fn main() {
    // 初始化日志系统 (env_logger)，允许通过 RUST_LOG 环境变量控制日志级别
    env_logger::init();
    
    // 初始化 Pingora Server 实例
    // Server 是 Pingora 的核心，负责管理工作线程、信号处理和平滑重启
    let mut server = Server::new(Some(Opt::default())).unwrap();
    server.bootstrap();

    // 创建一个独立的 Tokio Runtime
    // Pingora 内部有自己的 Runtime，但在启动 Pingora 之前，我们需要先用一个 Runtime 
    // 去连 Control Plane 拿配置。这也是 Data Plane 的 "Bootstrap" 过程。
    let rt = tokio::runtime::Runtime::new().unwrap();

    // 1. 获取 Control Plane 地址 (环境变量优先，默认本地)
    let cp_url = std::env::var("AGW_CONTROL_PLANE_URL")
        .unwrap_or_else(|_| "http://localhost:18000".to_string());
    println!(
        "Connecting to Control Plane at {} to fetch initial config...",
        cp_url
    );

    // 2.【同步阻塞】获取初始配置 (Initial Config Fetch)
    // 我们的策略是：必须拿到第一份有效配置，才能启动网关服务。
    // 如果连不上 Control Plane，或者拿到的是空配置，就死循环重试。
    let initial_config = rt.block_on(async {
        loop {
            // 尝试建立 gRPC 连接
            match AgwClient::connect(cp_url.clone(), "node-1".to_string()).await {
                Ok(mut client) => {
                    // 构造握手请求 (Node Identity)
                    let request = tonic::Request::new(client::Node {
                        id: "node-1".to_string(), // TODO: 应该动态生成或从配置读取
                        region: "us-east-1".to_string(),
                        version: "0.1.0".to_string(),
                    });
                    
                    // 发起 StreamConfig 请求
                    match client.client.stream_config(request).await {
                        Ok(resp) => {
                            // 获取从 Server 返回的流 (Stream)
                            let mut stream = resp.into_inner();
                            // 等待流里的第一条消息 (First Snapshot)
                            if let Ok(Some(snapshot)) = stream.message().await {
                                // 校验配置有效性：如果 Listener 为空，说明 Control Plane 可能还没准备好
                                if snapshot.listeners.is_empty() {
                                    eprintln!("Received config, but it has NO listeners (likely Control Plane is not ready). Retrying...");
                                } else {
                                    // 成功拿到有效配置！跳出循环，进入下一步
                                    return snapshot;
                                }
                            }
                        }
                        Err(e) => eprintln!("Stream handshake failed: {}", e),
                    }
                }
                Err(e) => eprintln!("Connection failed: {}", e),
            }
            // 失败重试，防止把 CPU 跑满
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        }
    });

    println!(
        "Received initial config version: {}",
        initial_config.version_id
    );

    // 【Why Clone?】
    // 这里我们使用了 `initial_config.clone()`，因为我们实际上需要把这份配置用两次：
    // 1. 第一次：放入 `config_store` (ArcSwap) 里，作为全局配置供 Proxy 处理请求使用。这一步会消耗掉数据的所有权。
    // 2. 第二次：在下面的 for 循环中，再次遍历 `initial_config.listeners`，把证书写到磁盘上。
    // 因此，我们需要克隆一份给 config_store。
    let config_store = Arc::new(ArcSwap::from_pointee(initial_config.clone()));

    let wasm_runtime = WasmRuntime::new();
    // 这个AgwProxy实现了一个trait ProxyHttp，Pingora会调用这个trait的
    let proxy_service = AgwProxy {
        config: config_store.clone(),
        wasm: wasm_runtime,
    };

    // 初始化 HTTP 代理服务
    // http_proxy_service 是 Pingora 提供的一个辅助函数，用于将我们的业务逻辑 (AgwProxy) 
    // 包装成一个标准的 Pingora Service。
    // 1. &server.configuration: 传入全局 server 配置（如线程数、PID 文件位置等）。
    // 2. proxy_service: 传入实现了 ProxyHttp Trait 的业务逻辑对象。
    let mut my_proxy = http_proxy_service(&server.configuration, proxy_service);

    // 2. Setup Listeners (根据初始配置启动端口监听)
    if initial_config.listeners.is_empty() {
        // Fallback: 如果万一没有 Listener，至少开个 HTTP 端口防止服务起不来
        my_proxy.add_tcp("0.0.0.0:6188");
    }


    // 遍历初始配置里的监听器 definition
    for listener in &initial_config.listeners {
        // 构造监听地址字符串，例如 "0.0.0.0:6188"
        let addr = format!("{}:{}", listener.address, listener.port);
        
        // 判断是否为 HTTPS/TLS 监听器
        if let Some(tls) = &listener.tls {
            // 【TLS 证书处理：写文件策略】
            // Pingora 的 `add_tls` 方法目前只支持传入证书文件的路径 (str)，
            // 不支持直接传入内存中的证书内容 (bytes)。
            // 而我们的证书是从 Control Plane 通过网络传过来的内存数据。
            // 解决方案：先把证书内容写到本地临时目录 (/tmp/) 下，再把文件路径传给 Pingora。
            let cert_path = format!("/tmp/{}_cert.pem", listener.name);
            let key_path = format!("/tmp/{}_key.pem", listener.name);

            // 1. 写证书文件 (public cert)
            if let Err(e) = std::fs::write(&cert_path, &tls.cert_pem) {
                eprintln!("Failed to write cert for {}: {}", listener.name, e);
                continue; // 写失败则跳过该端口监听，不影响其他端口
            }
            // 2. 写私钥文件 (private key)
            if let Err(e) = std::fs::write(&key_path, &tls.key_pem) {
                eprintln!("Failed to write key for {}: {}", listener.name, e);
                continue;
            }

            println!(
                "Adding TLS Listener: {} at {}. Cert: {} bytes, Key: {} bytes",
                listener.name,
                addr,
                tls.cert_pem.len(),
                tls.key_pem.len()
            );

            // 3. 注册 HTTPS 监听器
            // 这一步告诉 Pingora: "在 addr 这个端口上监听 HTTPS 流量，用这组证书解密"。
            if let Err(e) = my_proxy.add_tls(&addr, &cert_path, &key_path) {
                eprintln!("Failed to add TLS listener {}: {}", listener.name, e);
            }
        } else {
            // 【普通 TCP/HTTP 处理】
            println!("Adding TCP Listener: {} at {}", listener.name, addr);
            // 注册普通 TCP 监听器 (HTTP)
            my_proxy.add_tcp(&addr);
        }
    }

    // 3. 启动后台配置更新任务 (Spawn Background Update Task)
    // 我们的主线程 (main thread) 即将阻塞在 server.run_forever() 上，去处理 Pingora 的网络流量。
    // 所以我们需要起一个独立的线程 (std::thread::spawn)，专门负责“监听配置变更”。
    //
    // 注意：这里为什么不复用原本的 rt (Tokio Runtime)？
    // 因为 Pingora 启动后会接管所有的 Worker 线程，我们在外面起的线程需要自给自足，
    // 所以我们在后台线程里“新开”了一个 Tokio Runtime。
    let cp_url_bg = cp_url.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            loop {
                // 长连接重连逻辑
                match AgwClient::connect(cp_url_bg.clone(), "node-1".to_string()).await {
                    Ok(mut client) => {
                        let request = tonic::Request::new(client::Node {
                            id: "node-1".to_string(),
                            region: "us-east-1".to_string(),
                            version: "0.1.0".to_string(),
                        });
                        
                        // 建立 gRPC Stream
                        match client.client.stream_config(request).await {
                            Ok(resp) => {
                                let mut stream = resp.into_inner();
                                println!("Connected to CP stream (Background)...");
                                
                                // 【核心循环】：不断等待 Stream 里的新消息
                                while let Ok(Some(snapshot)) = stream.message().await {
                                    println!("Received Dynamic Config Update: Version {}", snapshot.version_id);
                                    
                                    // 【ArcSwap 写操作】
                                    // 这一步是最关键的：我们收到了 Control Plane 推过来的新配置。
                                    // 调用 store() 方法，"原子地" (Atomic) 替换掉全局指针。
                                    // 这一瞬间，所有新进来的 HTTP 请求就会立刻读到这份新配置。
                                    config_store.store(Arc::new(snapshot));
                                    
                                    // Note: Listeners update required restart in this MVP
                                }
                            }
                            Err(e) => eprintln!("Stream disconnected: {}", e),
                        }
                    }
                    Err(e) => eprintln!("Reconnect failed in background: {}", e),
                }
                // 断线重连等待 5 秒
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            }
        });
    });

    server.add_service(my_proxy);
    server.run_forever();
}
