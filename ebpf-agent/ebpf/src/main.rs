#![no_std]
#![no_main]
#![allow(static_mut_refs)]

use aya_ebpf::{
    bindings::{bpf_sock_ops, sk_action::SK_PASS},
    macros::{map, sk_msg, sock_ops},
    maps::SockHash,
    programs::{SkMsgContext, SockOpsContext},
    EbpfContext,
};
use aya_log_ebpf::info;
use ebpf_agent_common::PacketLog;

// =========================================================================================
// 核心数据结构：SockMap (SockHash)
// =========================================================================================
// 这是一个特殊的 BPF Map，类型为 BPF_MAP_TYPE_SOCKHASH。
// 作用：专门用于存储 TCP Socket 的内核引用（struct sock *）。
//
// Key: PacketLog
//   - 这是我们自定义的结构体，包含 (源IP, 目的IP, 源端口, 目的端口)。
//   - 它的作用是充当“查找索引”。
//
// Value: 隐式为 Socket 的文件描述符 (u32/fd)，在内核层面对应 socket 结构体。
//   - 当我们调用 map.update() 时，内核会自动把当前的 Socket 存进去。
//   - 当我们调用 redirect() 时，内核会根据 Key 找到对应的 Socket，把数据直接塞给它。
// =========================================================================================
#[map]
static mut SOCK_MAP: SockHash<PacketLog> = SockHash::with_max_entries(1024, 0);

// =========================================================================================
// 1. Hook点：sock_ops
// =========================================================================================
// 触发时机：Socket 状态发生变化时（例如建立连接、断开连接、重传等）。
// 这里的 `bpf_sockmap` 函数会挂载到 cgroup v2 的根目录（或特定目录）。
// 任何属于该 cgroup 的进程在创建 TCP 连接时，都会触发这个函数。
// =========================================================================================
#[sock_ops]
pub fn bpf_sockmap(ctx: SockOpsContext) -> u32 {
    match try_bpf_sockmap(ctx) {
        Ok(ret) => ret,
        Err(ret) => ret,
    }
}

fn try_bpf_sockmap(ctx: SockOpsContext) -> Result<u32, u32> {
    let op = ctx.op();

    // 过滤操作类型：我们只关心连接完全建立的时刻。
    // BPF_SOCK_OPS_PASSIVE_ESTABLISHED_CB = 4 (被动打开，服务端收到 ACK，连接 ESTABLISHED)
    // BPF_SOCK_OPS_ACTIVE_ESTABLISHED_CB = 5 (主动打开，客户端收到 SYN+ACK，连接 ESTABLISHED)
    if op != 4 && op != 5 {
        return Ok(0);
    }

    // 【构造 Key：四元组】
    // 这里的 Key 代表“当前 Socket 的身份”。
    // 例如：127.0.0.1:1234 (Client) <-> 127.0.0.1:8080 (Server)
    // 对于 Client 来说：Local=1234, Remote=8080
    // 对于 Server 来说：Local=8080, Remote=1234
    //
    // 注意：remote_port() 和 local_port() 返回的是网络字节序或主机字节序取决于上下文，
    // 但作为 Key 只要存取一致即可。这里强转 u16 是为了匹配 PacketLog 定义。
    let mut key = PacketLog {
        ipv4_src: ctx.remote_ip4(), // 对方 IP (远端)
        ipv4_dst: ctx.local_ip4(),  //己方 IP (本地)
        port_src: ctx.remote_port() as u16,
        port_dst: ctx.local_port() as u16,
        len: 0,
    };

    info!(
        &ctx,
        "Socket established: {:x}:{} -> {:x}:{}",
        key.ipv4_src,
        key.port_src,
        key.ipv4_dst,
        key.port_dst
    );

    // 【核心操作：更新 Map】
    // SOCK_MAP.update 将 "当前上下文对应的 Socket" 存入 Map，使用 `key` 作为索引。
    //
    // 效果：
    // 以后如果我们想把数据发给这个 Socket，只需要在 Map 里查找这个 `key` 即可。
    //
    // 注意：update 的第二个参数 `ops` 实际上就是当前的 Socket 上下文。
    unsafe {
        let ops = &mut *ctx.ops;
        SOCK_MAP.update(&mut key, ops, 0).map_err(|e| {
            info!(&ctx, "Failed to update SOCK_MAP: {}", e);
            1u32
        })?;
    }

    Ok(0)
}

// =========================================================================================
// 2. Hook点：sk_msg
// =========================================================================================
// 触发时机：应用程序调用 send()/sendmsg() 发送数据时。
// 这里的 `bpf_redirect` 函数会“附着”在 SOCK_MAP 上。
// 也就是说，只有当一个 Socket 被上面的 `sock_ops` 程序加入到 SOCK_MAP 后，
// 它的 sendmsg 操作才会被这个 `sk_msg` 程序拦截。
// =========================================================================================
#[sk_msg]
pub fn bpf_redirect(ctx: SkMsgContext) -> u32 {
    match try_bpf_redirect(ctx) {
        Ok(ret) => ret,
        Err(ret) => ret,
    }
}

fn try_bpf_redirect(ctx: SkMsgContext) -> Result<u32, u32> {
    // 获取原始消息结构体指针 (struct sk_msg_md *)
    let msg = ctx.msg;

    // 【构造查找 Key：寻找对端】
    // 我们的目标是把数据直接转发给“接收者”。
    //
    //当前场景：
    // 比如这是 Client 在发送数据。
    // msg.local_ip   = Client IP
    // msg.remote_ip  = Server IP
    //
    // 我们要找的是 Server 的 Socket。
    // 回忆 sock_ops 里 Server 是怎么存自己在 Map里的：
    // Server 存 Key 時：src = Remote(Client), dst = Local(Server)
    //
    // 所以，我们现在手里拿着 Client 的发包信息，要构造出 Server 当时存的 Key：
    // Key.src 应该是 Client IP (msg.local)
    // Key.dst 应该是 Server IP (msg.remote)
    //
    // 结论：
    // Key { src: msg.local, dst: msg.remote } 正好对应 Server 在 SOCK_MAP 里的 Key。
    let mut key = unsafe {
        PacketLog {
            ipv4_src: (*msg).local_ip4,
            ipv4_dst: (*msg).remote_ip4,
            port_src: (*msg).local_port as u16,
            port_dst: (*msg).remote_port as u16,
            len: 0,
        }
    };

    // 【核心黑科技：Socket 重定向】
    // bpf_msg_redirect_hash 尝试在 SOCK_MAP 中查找 `key`。
    //
    // 1. 查找成功：
    //    找到对应的 Socket (Server 的 Socket)。
    //    内核直接把当前这个 msg (数据包) 挂到目标 Socket 的 "接收队列" (Ingress Queue)。
    //    标志 BPF_F_INGRESS 的作用就是告诉内核："这虽然是发出来的数据，但请把它当作收到的数据处理"。
    //
    // 2. 效果：
    //    数据完全不经过 TCP/IP 协议栈（没有 IP 层路由、没有 TCP 层封包解包、没有 iptables 规则检查）。
    //    Server 进程醒来 read()，直接读到了数据，仿佛是光速传输过来的。
    //
    // 3. 查找失败：
    //    如果 Map 里没找到（比如 Server 不在同一个节点，或者 Server 还没建立连接），
    //    则返回 SK_PASS。
    //    结果：数据继续走正常的 TCP/IP 协议栈流程，保证了“退化兼容性”。
    unsafe {
        let _ = aya_ebpf::helpers::bpf_msg_redirect_hash(
            msg,
            core::ptr::addr_of_mut!(SOCK_MAP) as *mut _ as *mut _,
            &mut key as *mut _ as *mut _,
            aya_ebpf::bindings::BPF_F_INGRESS as u64,
        );
        // The helper returns the verdict (which is usually SK_PASS if successful or not, redirect flag is set in msg)
    }

    Ok(SK_PASS)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    unsafe { core::hint::unreachable_unchecked() }
}
