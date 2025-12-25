// 声明宿主(Host)提供的函数
// 这是一个 "FFI" (Foreign Function Interface) 声明。
// 告诉 Rust 编译器：虽然我没这个函数的源码，但宿主环境(Pingora)会提供它。
// 这里使用了 C 语言的调用约定 (extern "C")，因为 Wasm 的 ABI 是基于 C 的指针传递。
extern "C" {
    fn agw_get_header(
        name_ptr: *const u8,  // 入参指针 (字符串, const)
        name_len: usize,      // 入参长度
        value_ptr: *mut u8,   // 结果指针 (Buffer, mut)
        value_max_len: usize, // 结果 Buffer 最大容量
    ) -> i32; // 返回实际读到的长度
}

// 声明插件入口函数
// #[no_mangle]: 禁止编译器修改函数名，保证编译后的 Wasm 里函数名就是 "on_request"
// extern "C": 使用标准的 C 调用约定，因为 Host 是按 C 函数的方式来查找和调用的
#[no_mangle]
pub extern "C" fn on_request() -> i32 {
    let name = "user-agent";
    let mut value_buf = [0u8; 128]; // 在栈上申请 128 字节的空间作为 Buffer

    // 调用宿主函数 (Host Function)
    // 凡是涉及 FFI (外部函数调用) 的地方，Rust 都认为是 "Unsafe" 的，
    // 因为编译器无法保证外部代码的内存安全性，需要开发者手动担保。
    let len = unsafe {
        agw_get_header(
            name.as_ptr(),          // "user-agent" 的内存地址
            name.len(),             // 长度
            value_buf.as_mut_ptr(), // Buffer 的可写指针
            value_buf.len(),        // Buffer 的大小
        )
    };

    if len > 0 {
        // 从 Buffer 里还原字符串
        // slice::from_raw_parts 也是 unsafe 的，因为它直接操作裸指针
        let value = unsafe { std::slice::from_raw_parts(value_buf.as_ptr(), len as usize) };
        if let Ok(value_str) = std::str::from_utf8(value) {
            // 业务逻辑：如果 User-Agent 包含 "curl"，就拦截
            if value_str.contains("curl") {
                return 1; // 返回 1 表示 Deny (拦截)
            }
        }
    }

    // 默认放行 (或者没读到 Header)
    0 // 返回 0 表示 Allow (放行)
}
