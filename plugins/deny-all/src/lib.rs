extern "C" {
    fn agw_get_header(
        name_ptr: *const u8,
        name_len: usize,
        value_ptr: *mut u8,
        value_max_len: usize,
    ) -> i32;
}

#[no_mangle]
pub extern "C" fn on_request() -> i32 {
    let name = "user-agent";
    let mut value_buf = [0u8; 128]; // Max 128 bytes for UA

    let len = unsafe {
        agw_get_header(
            name.as_ptr(),
            name.len(),
            value_buf.as_mut_ptr(),
            value_buf.len(),
        )
    };

    if len > 0 {
        let value = unsafe { std::slice::from_raw_parts(value_buf.as_ptr(), len as usize) };
        if let Ok(value_str) = std::str::from_utf8(value) {
            // Logic: Block if User-Agent contains "curl"
            if value_str.contains("curl") {
                return 1; // Deny
            }
        }
    }

    // Allow by default (or if header missing)
    0
}
