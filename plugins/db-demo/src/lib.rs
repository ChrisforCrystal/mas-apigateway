use std::collections::HashMap;

#[link(wasm_import_module = "env")]
extern "C" {
    fn agw_get_header(
        name_ptr: *const u8,
        name_len: usize,
        value_ptr: *mut u8,
        value_max_len: usize,
    ) -> i32;

    fn agw_db_query(
        name_ptr: *const u8,
        name_len: usize,
        sql_ptr: *const u8,
        sql_len: usize,
        out_ptr: *mut u8,
        out_max: usize,
    ) -> i32;
}

#[no_mangle]
pub fn on_request() -> i32 {
    // 1. Get header to decide which DB to query (for demo purpose)
    // X-DB-Type: postgres | mysql
    let db_type = get_header("x-db-type");

    let (db_name, sql) = if db_type == "mysql" {
        ("products-mysql", "SELECT name FROM products LIMIT 1")
    } else {
        // Default to Postgres
        ("users-pg", "SELECT username FROM users LIMIT 1")
    };

    // 2. Execute Query
    let result = db_query(db_name, sql);

    // 3. Log result (in real world) or just check content
    if let Ok(json) = result {
        // If we got a result, we allow the request.
        // In a real auth plugin, we would check if the user exists.
        if json.len() > 2 {
            // "[]" is length 2
            return 0; // Allow
        }
    }

    // Default Allow for demo, or Deny if strict
    0
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

fn db_query(name: &str, sql: &str) -> Result<String, String> {
    let mut buf = [0u8; 2048]; // Larger buffer for JSON result
    let len = unsafe {
        agw_db_query(
            name.as_ptr(),
            name.len(),
            sql.as_ptr(),
            sql.len(),
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
