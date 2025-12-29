use wit_bindgen::generate;

generate!({
    path: "../../data-plane/wit/agw.wit",
    world: "plugin",
});

struct Plugin;

impl Guest for Plugin {
    fn handle_request(req_headers: Vec<(String, String)>) -> bool {
        // 【调用 Host 能力】
        // 下面这行代码，表面看是普通函数调用，
        // 实际上 WIT 会把它编译成 wait 指令，让 Host 去执行上面第二步里的代码。
        let user_id = req_headers
            .iter()
            .find(|(k, _)| k.to_lowercase() == "x-user-id")
            .map(|(_, v)| v.clone())
            .unwrap_or_default();

        if user_id.is_empty() {
            return true; // Allow
        }

        // 2. Call Redis: INCR user_id
        // Using generated bindings! No unsafe pointers!
        // mas::agw::redis::execute(addr, cmd, args)
        let redis_name = "default";

        // Redis command args: ["INCR", user_id]
        // Note: The interface defines 'command' as the verb, and 'args' as list<string>
        let cmd_verb = "INCR";
        let cmd_args = vec![user_id];

        let result = mas::agw::redis::execute(redis_name, cmd_verb, &cmd_args);

        // 3. Check limit
        match result {
            Ok(Ok(count_str)) => {
                if let Ok(count) = count_str.trim().parse::<i32>() {
                    if count > 5 {
                        return false; // Deny
                    }
                }
            }
            Ok(Err(e)) => {
                // Log error using WIT logging interface
                mas::agw::logging::log(
                    mas::agw::logging::Level::Error,
                    &format!("Redis error: {}", e),
                );
            }
            Err(e) => {
                // Transport/Runtime error
                mas::agw::logging::log(
                    mas::agw::logging::Level::Error,
                    &format!("Host call error: {}", e),
                );
            }
        }

        true // Allow by default if not denied above
    }
}

export!(Plugin);
