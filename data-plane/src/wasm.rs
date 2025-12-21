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
        linker
            .func_wrap(
                "env",
                "agw_get_header",
                |mut caller: Caller<'_, WasmContext>,
                 name_ptr: i32,
                 name_len: i32,
                 value_ptr: i32,
                 value_max_len: i32|
                 -> i32 {
                    let memory = match caller.get_export("memory") {
                        Some(Extern::Memory(mem)) => mem,
                        _ => return -1,
                    };

                    // Read Header Name
                    let name = {
                        let mut name_buf = vec![0u8; name_len as usize];
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

                    // Look up header
                    // We must define 'value' in a separate scope or clone it to end the borrow on caller.data()
                    let value_str = {
                        let ctx = caller.data();
                        match ctx.headers.get(&name.to_lowercase()) {
                            Some(v) => v.clone(),
                            None => return 0, // Not found
                        }
                    };

                    let value_bytes = value_str.as_bytes();
                    let len = value_bytes.len();

                    if len > value_max_len as usize {
                        return -1;
                    }

                    // Write Value (Mut borrow here is now safe)
                    if memory
                        .write(&mut caller, value_ptr as usize, value_bytes)
                        .is_err()
                    {
                        return -1;
                    }

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
    pub fn run_plugin(&self, path: &str, headers: HashMap<String, String>) -> Result<bool> {
        let module = self.get_module(path)?;

        // Create context
        let ctx = WasmContext { headers };
        let mut store = Store::new(&self.engine, ctx);

        // Instantiate using Linker to link host functions
        let instance = self.linker.instantiate(&mut store, &module)?;

        let on_request = instance.get_typed_func::<(), i32>(&mut store, "on_request")?;

        let result = on_request.call(&mut store, ())?;

        Ok(result == 0) // 0 = Allow
    }
}
