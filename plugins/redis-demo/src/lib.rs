use std::collections::HashMap;

#[link(wasm_import_module = "env")]
extern "C" {
    fn agw_get_header(
        name_ptr: *const u8,
        name_len: usize,
        value_ptr: *mut u8,
        value_max_len: usize,
    ) -> i32;

    fn agw_redis_command(
        name_ptr: *const u8,
        name_len: usize,
        cmd_ptr: *const u8,
        cmd_len: usize,
        out_ptr: *mut u8,
        out_max: usize,
    ) -> i32;
}

#[no_mangle]
pub fn on_request() -> i32 {
    // 1. Get Header "X-User-ID"
    let user_id = get_header("x-user-id");
    if user_id.is_empty() {
        return 0; // Allow if no user id
    }

    // 2. Call Redis: INCR user_id
    // Command: ["INCR", user_id]
    // JSON: ["INCR", "123"]
    let cmd_json = format!("[\"INCR\", \"{}\"]", user_id);
    let redis_name = "default"; // Assume configured name is "default"

    // [触发点]
    // 这一行调用会穿透到 Host (wasm.rs)
    // -> agw_redis_command
    // -> mem.read() 读取参数
    // -> redis::cmd(&args[0]) [对应 wasm.rs:188!]
    let result = redis_command(redis_name, &cmd_json);

    // 3. Check limit (e.g. > 5)
    if let Ok(count_str) = result {
        if let Ok(count) = count_str.trim().parse::<i32>() {
            if count > 5 {
                return 1; // Deny
            }
        }
    }

    0 // Allow
}

fn get_header(name: &str) -> String {
    let mut buf = [0u8; 128];
    let len = unsafe { agw_get_header(name.as_ptr(), name.len(), buf.as_mut_ptr(), buf.len()) };
    if len > 0 {
        String::from_utf8_lossy(&buf[..len as usize]).to_string()
    } else {
        String::new()
    }
}

fn redis_command(name: &str, cmd_json: &str) -> Result<String, String> {
    let mut buf = [0u8; 1024];
    let len = unsafe {
        agw_redis_command(
            name.as_ptr(),
            name.len(),
            cmd_json.as_ptr(),
            cmd_json.len(),
            buf.as_mut_ptr(),
            buf.len(),
        )
    };
    if len >= 0 {
        Ok(String::from_utf8_lossy(&buf[..len as usize]).to_string())
    } else {
        Err(format!("Error code: {}", len))
    }
}
