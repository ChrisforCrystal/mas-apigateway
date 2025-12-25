use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};
use wasmtime::*;

use redis::Client as RedisClient;
use sqlx::{MySql, Pool, Postgres};

#[derive(Clone, Default)]
pub struct ExternalResources {
    pub redis: HashMap<String, RedisClient>,
    // For now support Postgres and MySQL. In real world, use AnyPool or enum
    pub postgres: HashMap<String, Pool<Postgres>>,
    pub mysql: HashMap<String, Pool<MySql>>,
}

pub struct WasmContext {
    pub headers: HashMap<String, String>,
    pub resources: ExternalResources,
}

#[derive(Clone)]
pub struct WasmRuntime {
    engine: Engine,
    // Cache compiled modules: Path -> Module
    // wasmtime::Module is cheap to clone (internal ref counting)
    modules: Arc<RwLock<HashMap<String, Module>>>,
    linker: Linker<WasmContext>,
    resources: ExternalResources,
}

impl WasmRuntime {
    pub fn new(resources: ExternalResources) -> Self {
        let mut config = Config::new();
        config.async_support(true);
        let engine = Engine::new(&config).unwrap();
        let mut linker = Linker::new(&engine);

        // Define Host Function: agw_get_header
        // (name_ptr, name_len, value_ptr, value_max_len) -> i32
        // 这个函数是宿主机(Host)暴露给 Wasm 插件的唯一能力接口。
        // 插件可以通过调用这个函数，从宿主机(Pingora)获取当前请求的 HTTP Header。
        linker
            .func_wrap(
                "env", // 模块名，通常 Wasm 默认导入 env 模块
                "agw_get_header",
                |mut caller: Caller<'_, WasmContext>,
                 name_ptr: i32,      // 参数1: Header Key 在 Wasm 内存中的起始地址
                 name_len: i32,      // 参数2: Header Key 的长度
                 value_ptr: i32,     // 参数3: 此时存放结果的 Buffer 在 Wasm 内存中的起始地址
                 value_max_len: i32| // 参数4: Buffer 的最大容量
                 -> i32 {
                    // 1. 获取 Wasm 的线性内存 (Linear Memory)
                    // 因为 Wasm 和 Host 的内存是隔离的，我们需要通过 export 获取 Wasm 内存的句柄，
                    // 才能读取它传过来的 Key，或者把 Value 写回给它。
                    let memory = match caller.get_export("memory") {
                        Some(Extern::Memory(mem)) => mem,
                        _ => return -1,
                    };

                    // 2.【读】读取 Header Name (从 Wasm 内存 -> Rust 字符串)
                    let name = {
                        let mut name_buf = vec![0u8; name_len as usize];
                        // memory.read: 从 Wasm 内存的 name_ptr 处读取 name_len 个字节
                        if memory
                            .read(&caller, name_ptr as usize, &mut name_buf)
                            .is_err()
                        {
                            return -1;
                        }
                        match String::from_utf8(name_buf) {
                            Ok(n) => n,
                            Err(_) => return -1,
                        }
                    };

                    // 3.【查】在 Rust 宿主机上下文中查找 Header 值
                    // caller.data() 获取我们在实例化 Wasm 时传入的上下文 (WasmContext)
                    let value_str = {
                        let ctx = caller.data();
                        match ctx.headers.get(&name.to_lowercase()) {
                            Some(v) => v.clone(), // 找到值，克隆出来（因为要结束借用）
                            None => return 0, // 没找到，返回 0 (表示空)
                        }
                    };

                    let value_bytes = value_str.as_bytes();
                    let len = value_bytes.len();

                    // 检查 Buffer 是否够大
                    if len > value_max_len as usize {
                        return -1; // 缓冲区溢出错误
                    }

                    // 4.【写】把结果写回 Wasm 内存 (Rust 字符串 -> Wasm 内存)
                    if memory
                        .write(&mut caller, value_ptr as usize, value_bytes)
                        .is_err()
                    {
                        return -1;
                    }

                    // 返回实际写入的字节数
                    len as i32
                },
            )
            .unwrap();

        // Host Function: agw_redis_command
        // (name_ptr, name_len, cmd_ptr, cmd_len, out_ptr, out_max) -> i32
        linker
            .func_wrap6_async(
                "env",
                "agw_redis_command",
                |mut caller: Caller<'_, WasmContext>,
                 name_ptr: i32,
                 name_len: i32,
                 cmd_ptr: i32,
                 cmd_len: i32,
                 out_ptr: i32,
                 out_max: i32| {
                    Box::new(async move {
                        // 1. Get Memory (Needs to be done inside async block? No, caller is moved)
                        let mem = match caller.get_export("memory") {
                            Some(Extern::Memory(mem)) => mem,
                            _ => return Ok(-1),
                        };

                        // 2. Read Name
                        // 2.【读取 Redis 实例名称】
                        // Wasm 传递的是指针 (name_ptr) 和长度 (name_len)。
                        // 我们需要从 Wasm 的内存空间 (mem) 中把这段字节读出来，转换成 Rust 的 String。
                        // 例如: "default" 或 "cache-redis"
                        let name = {
                            let mut buf = vec![0u8; name_len as usize];
                            // mem.read() 可能会失败（例如指针越界），如果失败返回错误码 -1
                            if mem.read(&caller, name_ptr as usize, &mut buf).is_err() {
                                return Ok(-1);
                            }
                            String::from_utf8(buf).unwrap_or_default()
                        };

                        // 3.【读取 Redis 命令 (JSON)】
                        // 同样的方式读取命令字符串。为了通用性，我们约定命令以 JSON 数组格式传递。
                        // 例如: ["SET", "mykey", "123"] 或 ["INCR", "counter"]
                        let cmd_json = {
                            let mut buf = vec![0u8; cmd_len as usize];
                            // 读取失败返回错误码 -2
                            if mem.read(&caller, cmd_ptr as usize, &mut buf).is_err() {
                                return Ok(-2);
                            }
                            buf
                        };

                        // 4.【解析 JSON 命令】
                        // 将 JSON 字节流反序列化为字符串数组 Vec<String>。
                        // 如果 JSON 格式不对，返回错误码 -3。
                        let args: Vec<String> = match serde_json::from_slice(&cmd_json) {
                            Ok(v) => v,
                            Err(_) => return Ok(-3),
                        };
                        // 空数组也不行，至少得有个命令名 (如 "GET")
                        if args.is_empty() {
                            return Ok(-3);
                        }

                        // 5.【获取 Redis 连接】
                        // 这一步是最关键的“资源查找”。
                        // caller.data() 获取我们在 run_plugin 里传入的 WasmContext。
                        // ctx.resources.redis 是一个 HashMap，存着所有预先初始化好的 Redis Client。
                        let mut conn = {
                            let ctx = caller.data();

                            // 根据名字 ("default") 查找对应的 Client
                            if let Some(client) = ctx.resources.redis.get(&name) {
                                // 获取一个异步连接 (MultiplexedConnection)。
                                // 这种连接是多路复用的，非常适合高并发场景。
                                match client.get_multiplexed_async_connection().await {
                                    Ok(c) => c,
                                    Err(_) => return Ok(-5), // 连接失败返回 -5
                                }
                            } else {
                                return Ok(-4); // 没找到叫这个名字的 Redis，返回 -4
                            }
                        };

                        // 6.【执行 Redis 命令】
                        // args[0] 是命令动词 (如 "SET", "GET")
                        let mut cmd = redis::cmd(&args[0]);
                        // args[1..] 是参数 (如 "key", "value")
                        for arg in &args[1..] {
                            cmd.arg(arg);
                        }

                        // 执行异步查询。
                        // 这里我们偷了个懒，强制把所有返回值都当作 String 处理。
                        // 实际 Redis 可能返回 Int, BulkString, Array 等。
                        //    let cmd_json = format!("[\"INCR\", \"{}\"]", user_id);

                        // MVP 阶段这样写能覆盖大部分 SET/GET/INCR 场景。
                        let result: redis::RedisResult<String> = cmd.query_async(&mut conn).await;

                        let resp_bytes = match result {
                            Ok(s) => s.into_bytes(),
                            Err(e) => format!("ERR: {}", e).into_bytes(), // 执行出错返回 "ERR: ..."
                        };

                        // 7.【写入返回结果】
                        // 检查 Wasm 提供的缓冲区 (out_max) 是否够大。
                        if resp_bytes.len() > out_max as usize {
                            return Ok(-6); // 缓冲区太小，装不下结果，返回 -6
                        }
                        // 把结果写回 Wasm 内存的 out_ptr 处
                        if mem
                            .write(&mut caller, out_ptr as usize, &resp_bytes)
                            .is_err()
                        {
                            return Ok(-7); // 写入内存失败返回 -7
                        }

                        // 成功！返回实际写入的字节数 (正数)
                        Ok(resp_bytes.len() as i32)
                    })
                },
            )
            .unwrap();

        // Host Function: agw_db_query
        linker
            .func_wrap6_async(
                "env",
                "agw_db_query",
                |mut caller: Caller<'_, WasmContext>,
                 name_ptr: i32,
                 name_len: i32,
                 sql_ptr: i32,
                 sql_len: i32,
                 out_ptr: i32,
                 out_max: i32| {
                    Box::new(async move {
                        let mem = match caller.get_export("memory") {
                            Some(Extern::Memory(mem)) => mem,
                            _ => return Ok(-1),
                        };

                        let name = {
                            let mut buf = vec![0u8; name_len as usize];
                            if mem.read(&caller, name_ptr as usize, &mut buf).is_err() {
                                return Ok(-1);
                            }
                            String::from_utf8(buf).unwrap_or_default()
                        };
                        let sql = {
                            let mut buf = vec![0u8; sql_len as usize];
                            if mem.read(&caller, sql_ptr as usize, &mut buf).is_err() {
                                return Ok(-2);
                            }
                            String::from_utf8(buf).unwrap_or_default()
                        };

                        let ctx = caller.data();
                        let result_json = if let Some(pool) = ctx.resources.postgres.get(&name) {
                            use sqlx::Row;
                            match sqlx::query(&sql).fetch_all(pool).await {
                                Ok(rows) => {
                                    let mut results = Vec::new();
                                    for row in rows {
                                        // Expecting first column to be convertible to String
                                        match row.try_get::<String, _>(0) {
                                            Ok(val) => results.push(val),
                                            Err(_) => return Ok(-8),
                                        }
                                    }
                                    serde_json::to_string(&results).unwrap_or_default()
                                }
                                Err(_) => return Ok(-5),
                            }
                        } else if let Some(pool) = ctx.resources.mysql.get(&name) {
                            use sqlx::Row;
                            match sqlx::query(&sql).fetch_all(pool).await {
                                Ok(rows) => {
                                    let mut results = Vec::new();
                                    for row in rows {
                                        match row.try_get::<String, _>(0) {
                                            Ok(val) => results.push(val),
                                            Err(_) => return Ok(-8),
                                        }
                                    }
                                    serde_json::to_string(&results).unwrap_or_default()
                                }
                                Err(_) => return Ok(-5),
                            }
                        } else {
                            return Ok(-4);
                        };

                        let resp_bytes = result_json.into_bytes();
                        if resp_bytes.len() > out_max as usize {
                            return Ok(-6);
                        }
                        if mem
                            .write(&mut caller, out_ptr as usize, &resp_bytes)
                            .is_err()
                        {
                            return Ok(-7);
                        }
                        Ok(resp_bytes.len() as i32)
                    })
                },
            )
            .unwrap();

        Self {
            engine,
            modules: Arc::new(RwLock::new(HashMap::new())),
            linker,
            resources,
        }
    }

    // Get or load a module from path
    pub fn get_module(&self, path: &str) -> Result<Module> {
        // Read lock first
        {
            let cache = self.modules.read().unwrap();
            if let Some(m) = cache.get(path) {
                return Ok(m.clone());
            }
        }

        // Load from disk
        // Note: verify path security in real world!
        if !Path::new(path).exists() {
            return Err(Error::msg(format!("Wasm file not found: {}", path)));
        }

        let module = Module::from_file(&self.engine, path)?;

        // Write lock to cache
        {
            let mut cache = self.modules.write().unwrap();
            cache.insert(path.to_string(), module.clone());
        }

        Ok(module)
    }

    // Execute the plugin. Returns true if request should continue (Allow), false if Deny.
    // MVP ABI: on_request() -> i32 (0=Allow, 1=Deny)
    // 执行 Wasm 插件的主逻辑
    // 返回值:
    // - Ok(true):  Allow, 请求继续
    // - Ok(false): Deny,  请求被拦截
    // - Err(...):  Error, 插件执行出错
    pub async fn run_plugin(&self, path: &str, headers: HashMap<String, String>) -> Result<bool> {
        let module = self.get_module(path)?;

        let ctx = WasmContext {
            headers,
            resources: self.resources.clone(),
        };

        // 3. 创建 Store (Wasm 实例的独立“宇宙”)
        // Store 包含了实例的所有运行时状态（内存、全局变量、Table 等），以及我们塞进去的 ctx。
        // 注意：每个请求的执行都需要一个新的临时 Store，用完即毁。
        let mut store = Store::new(&self.engine, ctx);

        // 4. 实例化 (Instantiation)
        // 把“蓝图” (Module) 变成“房子” (Instance)。
        // 关键点：使用 Linker 把 Host Function (宿主能力) 链接进这个实例。
        // 这样 Wasm 代码里调用的 "env.agw_get_header" 才能找到对应的 Rust 实现。
        let instance = self.linker.instantiate(&mut store, &module)?;

        // get_typed_func 会检查类型签名是否匹配。
        // 5. 查找并绑定入口函数 "on_request"
        // instance.get_typed_func::<Params, Return>(&mut store, "name")
        // - <(), i32>: 泛型参数，指定了函数的“形状” (Signature)。
        //   - (): 表示这个 Wasm 函数不接受任何参数 (Input)。
        //   - i32: 表示这个 Wasm 函数会返回一个 32 位整数 (Output)。
        // - "on_request": Wasm 模块里 export 出来的函数名。
        //
        // 这一步类似于“强类型转换”。如果 Wasm 里确实有这个函数，但它实际上需要传参数，
        // 或者返回的不是 i32，这里就会直接报错，防止后面调用时出现内存错误。
        let on_request = instance.get_typed_func::<(), i32>(&mut store, "on_request")?;

        // 6. 真正执行 Wasm 代码
        // call_async() 会非阻塞地执行，允许 Wasm 在调用 Host Function 时 yield
        let result = on_request.call_async(&mut store, ()).await?;

        // 约定：返回 0 表示放行 (Allow)，非 0 表示拦截 (Deny)
        Ok(result == 0)
    }
}
