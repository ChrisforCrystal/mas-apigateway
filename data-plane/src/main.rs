use async_trait::async_trait;
use pingora::proxy::ProxyHttp;
use pingora::proxy::Session;
use pingora::proxy::http_proxy_service;
use pingora::server::Server;
use pingora::server::configuration::Opt;
use std::sync::Arc;
use tokio::sync::Mutex;

mod client;
use client::AgwClient;

pub struct AgwProxy;

#[async_trait]
impl ProxyHttp for AgwProxy {
    type CTX = ();
    fn new_ctx(&self) -> Self::CTX {
        ()
    }

    async fn upstream_peer(
        &self,
        _session: &mut Session,
        _ctx: &mut Self::CTX,
    ) -> pingora::Result<Box<pingora::upstreams::peer::HttpPeer>> {
        let addr = ("1.1.1.1", 80);
        let peer = Box::new(pingora::upstreams::peer::HttpPeer::new(
            addr,
            false,
            "one.one.one.one".to_string(),
        ));
        Ok(peer)
    }
}

fn main() {
    let mut server = Server::new(Some(Opt::default())).unwrap();
    server.bootstrap();

    // US3: Start gRPC Client in background
    // pingora's run_forever blocks, so we spawn before.
    // However, pingora uses its own runtime? No, we can use tokio::spawn if we are in a runtime.
    // pingora `main` usually sets up runtime?
    // Server::bootstrap initializes stuff.
    // We can spawn a separate thread or use server.run_forever (which runs the event loop).
    // Better: spawn a background task?
    // Pingora services can be "background services".
    // For MVP, let's just spawn a tokio thread if we can, or just print log in main for now?
    // Wait, we need to run the client.
    // We can use `tokio::runtime::Runtime` or just rely on Pingora's runtime if we can hook into it.
    // Pingora supports `add_service`. We can wrap our client as a service?
    // Let's keep it simple: spawn a tokio task inside `main` (but main isn't async).
    // We can use a separate thread for the client loop.

    std::thread::spawn(|| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            // Retry loop
            loop {
                match AgwClient::connect("http://localhost:18000".to_string(), "node-1".to_string())
                    .await
                {
                    Ok(mut client) => {
                        if let Err(e) = client.start_stream().await {
                            eprintln!("Stream error: {}", e);
                        }
                    }
                    Err(e) => {
                        eprintln!("Connection failed: {}", e);
                    }
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            }
        });
    });

    let mut my_proxy = http_proxy_service(&server.configuration, AgwProxy);
    my_proxy.add_tcp("0.0.0.0:6188");

    println!("AGW Data Plane starting on 0.0.0.0:6188");
    server.add_service(my_proxy);
    server.run_forever();
}
