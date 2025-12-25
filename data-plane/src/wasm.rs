use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};
use wasmtime::*;

pub struct WasmContext {
    pub headers: HashMap<String, String>,
}

#[derive(Clone)]
pub struct WasmRuntime {
    engine: Engine,
    // Cache compiled modules: Path -> Module
    // wasmtime::Module is cheap to clone (internal ref counting)
    modules: Arc<RwLock<HashMap<String, Module>>>,
    linker: Linker<WasmContext>,
}

impl WasmRuntime {
    pub fn new() -> Self {
        let engine = Engine::default();
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

        Self {
            engine,
            modules: Arc::new(RwLock::new(HashMap::new())),
            linker,
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
    pub fn run_plugin(&self, path: &str, headers: HashMap<String, String>) -> Result<bool> {
        // 1. 获取已编译好的 Wasm Module
        // 如果缓存里有就直接拿，没有就从磁盘加载并编译
        let module = self.get_module(path)?;

        // 2. 创建上下文 (Context)
        // 这里的 ctx 包含了本次请求的所有 Header 信息。
        // 它将被存入下面创建的 Store 中，供 Host Function (如 agw_get_header) 使用。
        let ctx = WasmContext { headers };

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
        // call() 会阻塞当前线程，直到 Wasm 执行完毕或陷阱 (Trap)。
        let result = on_request.call(&mut store, ())?;

        // 约定：返回 0 表示放行 (Allow)，非 0 表示拦截 (Deny)
        Ok(result == 0)
    }
}
