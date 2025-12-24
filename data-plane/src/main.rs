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

    async fn request_filter(
        &self,
        session: &mut Session,
        _ctx: &mut Self::CTX,
    ) -> pingora::Result<bool> {
        // Dynamic Routing Logic
        let config = self.config.load();
        let path = session.req_header().uri.path();
        let _host = session.req_header().uri.host().unwrap_or("");

        // Simple loop matching (Enhancement: use Trie or HashMpa)
        for route in &config.routes {
            // Prefix match
            if path.starts_with(&route.path_prefix) {
                // Check Plugins
                if !route.plugins.is_empty() {
                    // Extract Headers for Wasm
                    let mut headers = std::collections::HashMap::new();
                    for (name, value) in session.req_header().headers.iter() {
                        if let Ok(v_str) = value.to_str() {
                            headers.insert(name.to_string(), v_str.to_string());
                        }
                    }

                    for plugin in &route.plugins {
                        match self.wasm.run_plugin(&plugin.wasm_path, headers.clone()) {
                            Ok(allow) => {
                                if !allow {
                                    // Plugin Denied
                                    let _ = session.respond_error(403).await;
                                    return Ok(true); // Handled
                                }
                            }
                            Err(e) => {
                                eprintln!("Wasm Plugin Error [{}]: {}", plugin.name, e);
                                let _ = session.respond_error(500).await;
                                return Ok(true);
                            }
                        }
                    }
                }
                // Store selected cluster in session state (or just header for now)
                // We need to pass state to upstream_peer.
                // Pingora allows passing state via CTX or modifying header.
                // Let's attach a custom header "x-agw-cluster" for internal use?
                // Or better, ProxyHttp doesn't easily share state between request_filter and upstream_peer unless it's in CTX.
                // But CTX is () right now.
                // Let's implement simple round-robin or first endpoint of the cluster here?
                // Wait, upstream_peer needs to return a Peer.
                // We should resolve cluster here and put it in CTX.
                return Ok(false); // Continue to upstream_peer
            }
        }

        // No match? 404
        // Pingora doesn't easily return 404 from here without sending response manually.
        // We can return true (handled) after sending error.
        let _ = session.respond_error(404).await;
        Ok(true)
    }

    async fn upstream_peer(
        &self,
        session: &mut Session,
        _ctx: &mut Self::CTX,
    ) -> pingora::Result<Box<pingora::upstreams::peer::HttpPeer>> {
        // Re-match or use CTX. For now re-match (inefficient but safe).
        let config = self.config.load();
        let path = session.req_header().uri.path();

        let mut cluster_name = "";
        for route in &config.routes {
            if path.starts_with(&route.path_prefix) {
                cluster_name = &route.cluster_id;
                break;
            }
        }

        if cluster_name.is_empty() {
            return Err(pingora::Error::create(
                pingora::ErrorType::HTTPStatus(502),
                pingora::ErrorSource::Upstream,
                Some("no route match".into()),
                None,
            ));
        }

        // Find cluster
        let cluster = config.clusters.iter().find(|c| c.name == cluster_name);
        if let Some(c) = cluster {
            if let Some(endpoint) = c.endpoints.first() {
                // Simple first endpoint
                let addr = (endpoint.address.as_str(), endpoint.port as u16);
                // Using IP address needs parsing if it's string.
                // Pingora HttpPeer takes a SocketAddr or string?
                // It takes logic.
                // HttpPeer::new(address, tls, sni)
                // address can be ToSocketAddrs? No, it's (A, u16).
                // We need to support DNS? Assuming IP now for MVP.
                let peer = Box::new(pingora::upstreams::peer::HttpPeer::new(
                    addr,
                    false,
                    "".to_string(),
                ));
                return Ok(peer);
            }
        }

        Err(pingora::Error::create(
            pingora::ErrorType::HTTPStatus(503),
            pingora::ErrorSource::Upstream,
            Some("no healthy endpoint".into()),
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

    // Shared Config State
    let config_store = Arc::new(ArcSwap::from_pointee(initial_config.clone()));

    let wasm_runtime = WasmRuntime::new();
    let proxy_service = AgwProxy {
        config: config_store.clone(),
        wasm: wasm_runtime,
    };

    let mut my_proxy = http_proxy_service(&server.configuration, proxy_service);

    // 2. Setup Listeners
    if initial_config.listeners.is_empty() {
        // Fallback for safety/testing if no listeners defined
        my_proxy.add_tcp("0.0.0.0:6188");
    }

    for listener in &initial_config.listeners {
        let addr = format!("{}:{}", listener.address, listener.port);
        if let Some(tls) = &listener.tls {
            // Write cert/key to temp files
            let cert_path = format!("/tmp/{}_cert.pem", listener.name);
            let key_path = format!("/tmp/{}_key.pem", listener.name);

            if let Err(e) = std::fs::write(&cert_path, &tls.cert_pem) {
                eprintln!("Failed to write cert for {}: {}", listener.name, e);
                continue;
            }
            if let Err(e) = std::fs::write(&key_path, &tls.key_pem) {
                eprintln!("Failed to write key for {}: {}", listener.name, e);
                continue;
            }

            println!(
                "Adding TLS Listener: {} at {}. Cert: {} bytes, Key: {} bytes (Using MANUAL paths for debug)",
                listener.name,
                addr,
                tls.cert_pem.len(),
                tls.key_pem.len()
            );

            if let Err(e) = my_proxy.add_tls(&addr, &cert_path, &key_path) {
                eprintln!("Failed to add TLS listener {}: {}", listener.name, e);
            }
        } else {
            println!("Adding TCP Listener: {} at {}", listener.name, addr);
            my_proxy.add_tcp(&addr);
        }
    }

    // 3. Spawn Background Update Task
    let cp_url_bg = cp_url.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            loop {
                // Determine current version to avoid re-fetching same?
                // For now, simple stream update
                match AgwClient::connect(cp_url_bg.clone(), "node-1".to_string()).await {
                    Ok(mut client) => {
                        let request = tonic::Request::new(client::Node {
                            id: "node-1".to_string(),
                            region: "us-east-1".to_string(),
                            version: "0.1.0".to_string(),
                        });
                        match client.client.stream_config(request).await {
                            Ok(resp) => {
                                let mut stream = resp.into_inner();
                                println!("Connected to CP stream...");
                                while let Ok(Some(snapshot)) = stream.message().await {
                                    println!("Applied Config Version: {}", snapshot.version_id);
                                    config_store.store(Arc::new(snapshot));
                                    // Note: Listeners update required restart in this MVP
                                }
                            }
                            Err(e) => eprintln!("Stream failed: {}", e),
                        }
                    }
                    Err(e) => eprintln!("Reconnect failed: {}", e),
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            }
        });
    });

    server.add_service(my_proxy);
    server.run_forever();
}
