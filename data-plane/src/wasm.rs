use async_trait::async_trait;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};
use wasmtime::component::*;
use wasmtime::{Config, Engine, Store};

use redis::Client as RedisClient;
use sqlx::{MySql, Pool, Postgres};

// 1. 魔法宏：bindgen!
// 这个宏会读取 .wit 文件，自动生成一堆 Rust trait 代码。
// 比如它会生成一个 trait mas::agw::logging::Host
// Generate traits from WIT
bindgen!({
    path: "wit/agw.wit",
    world: "plugin", // 指定我们要实现哪个 world
    async: true,  // 开启异步支持（关键！）
});

#[derive(Clone, Default)]
pub struct ExternalResources {
    pub redis: HashMap<String, RedisClient>,
    pub postgres: HashMap<String, Pool<Postgres>>,
    pub mysql: HashMap<String, Pool<MySql>>,
}

pub struct WasmContext {
    pub headers: HashMap<String, String>,
    pub resources: ExternalResources,
    // Required by wasi if we use it, but for now we are custom
    // wasi_ctx: WasiCtx,
}

// 2. 实现生成的 Trait
// 这里的 WasmContext 就是那个承载所有状态的结构体

#[async_trait]
impl mas::agw::logging::Host for WasmContext {
    async fn log(&mut self, lvl: mas::agw::logging::Level, msg: String) -> wasmtime::Result<()> {
        match lvl {
            mas::agw::logging::Level::Debug => println!("DEBUG [Plugin]: {}", msg),
            mas::agw::logging::Level::Info => println!("INFO [Plugin]: {}", msg),
            mas::agw::logging::Level::Warn => println!("WARN [Plugin]: {}", msg),
            mas::agw::logging::Level::Error => println!("ERROR [Plugin]: {}", msg),
        }
        Ok(())
    }
}

#[async_trait]
impl mas::agw::redis::Host for WasmContext {
    async fn execute(
        &mut self,
        addr: String,
        command: String,
        args: Vec<String>,
    ) -> wasmtime::Result<Result<String, String>> {
        // Find Redis client
        let client = match self.resources.redis.get(&addr) {
            Some(c) => c,
            None => return Ok(Err(format!("Redis resource '{}' not found", addr))),
        };

        // Get connection
        let mut conn = match client.get_multiplexed_async_connection().await {
            Ok(c) => c,
            Err(e) => return Ok(Err(format!("Failed to connect to Redis: {}", e))),
        };

        // Build command
        let mut cmd = redis::cmd(&command);
        for arg in args {
            cmd.arg(arg);
        }

        // Execute
        let result: redis::RedisResult<String> = cmd.query_async(&mut conn).await;
        match result {
            Ok(v) => Ok(Ok(v)),
            Err(e) => Ok(Err(format!("Redis error: {}", e))),
        }
    }
}

#[async_trait]
impl mas::agw::database::Host for WasmContext {
    async fn query(
        &mut self,
        db_type: mas::agw::database::DbType,
        connection: String,
        sql: String,
    ) -> wasmtime::Result<Result<String, String>> {
        use sqlx::Row;

        let json_result = match db_type {
            mas::agw::database::DbType::Postgres => {
                let pool = match self.resources.postgres.get(&connection) {
                    Some(p) => p,
                    None => {
                        return Ok(Err(format!("Postgres resource '{}' not found", connection)));
                    }
                };
                match sqlx::query(&sql).fetch_all(pool).await {
                    Ok(rows) => {
                        let mut results = Vec::new();
                        for row in rows {
                            // Simple mapping: assume first column is string-able
                            // In a real system, we'd map the whole row to JSON
                            let val: String = row.try_get(0).unwrap_or_default();
                            results.push(val);
                        }
                        serde_json::to_string(&results).unwrap_or_default()
                    }
                    Err(e) => return Ok(Err(format!("Postgres query failed: {}", e))),
                }
            }
            mas::agw::database::DbType::Mysql => {
                let pool = match self.resources.mysql.get(&connection) {
                    Some(p) => p,
                    None => return Ok(Err(format!("MySQL resource '{}' not found", connection))),
                };
                match sqlx::query(&sql).fetch_all(pool).await {
                    Ok(rows) => {
                        let mut results = Vec::new();
                        for row in rows {
                            let val: String = row.try_get(0).unwrap_or_default();
                            results.push(val);
                        }
                        serde_json::to_string(&results).unwrap_or_default()
                    }
                    Err(e) => return Ok(Err(format!("MySQL query failed: {}", e))),
                }
            }
        };

        Ok(Ok(json_result))
    }
}

#[derive(Clone)]
pub struct WasmRuntime {
    engine: Engine,
    // Cache compiled components: Path -> Component
    components: Arc<RwLock<HashMap<String, Component>>>,
    linker: Linker<WasmContext>,
    resources: ExternalResources,
}

impl WasmRuntime {
    pub fn new(resources: ExternalResources) -> Self {
        let mut config = Config::new();
        config.async_support(true);
        config.wasm_component_model(true); // Enable Component Model

        let engine = Engine::new(&config).unwrap();
        let mut linker = Linker::new(&engine);

        // Link host functions defined in Plugin::add_to_linker
        // This generates the glue code to register our Host trait implementations
        Plugin::add_to_linker(&mut linker, |ctx| ctx).unwrap();

        Self {
            engine,
            components: Arc::new(RwLock::new(HashMap::new())),
            linker,
            resources,
        }
    }

    pub fn get_component(&self, path: &str) -> wasmtime::Result<Component> {
        {
            let cache = self.components.read().unwrap();
            if let Some(c) = cache.get(path) {
                return Ok(c.clone());
            }
        }

        if !Path::new(path).exists() {
            return Err(wasmtime::Error::msg(format!(
                "Wasm file not found: {}",
                path
            )));
        }

        // Compile component from file
        let component = Component::from_file(&self.engine, path)?;

        {
            let mut cache = self.components.write().unwrap();
            cache.insert(path.to_string(), component.clone());
        }

        Ok(component)
    }

    pub async fn run_plugin(
        &self,
        path: &str,
        headers: HashMap<String, String>,
    ) -> wasmtime::Result<bool> {
        let component = self.get_component(path)?;

        let ctx = WasmContext {
            headers,
            resources: self.resources.clone(),
        };

        let mut store = Store::new(&self.engine, ctx);

        // Instantiate
        let (bindings, _) = Plugin::instantiate_async(&mut store, &component, &self.linker).await?;

        // Call handle-request
        // Convert HashMap headers to Vec<(String, String)> for WIT list<tuple<string, string>>
        let req_headers: Vec<(String, String)> = store.data().headers.clone().into_iter().collect();

        let result = bindings
            .call_handle_request(&mut store, &req_headers)
            .await?;

        Ok(result)
    }
}
