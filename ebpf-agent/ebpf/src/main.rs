#![no_std]
#![no_main]

use aya_ebpf::{
    bindings::bpf_sock_ops,
    macros::{map, sk_msg, sock_ops},
    maps::SockHash,
    programs::{SkMsgContext, SockOpsContext},
    EbpfContext,
};
use aya_log_ebpf::info;
use ebpf_agent_common::PacketLog;

#[map]
static mut SOCK_MAP: SockHash<PacketLog, u32> = SockHash::with_max_entries(1024, 0);

/// 1. sock_ops: Intercept socket establishment
/// This program runs when TCP state changes. We use it to populate the SOCK_MAP.
/// When a new connection is established, we save the socket into the map using a key
/// derived from the 4-tuple (src_ip, dst_ip, src_port, dst_port).
#[sock_ops]
pub fn bpf_sockmap(ctx: SockOpsContext) -> u32 {
    match try_bpf_sockmap(ctx) {
        Ok(ret) => ret,
        Err(ret) => ret,
    }
}

fn try_bpf_sockmap(ctx: SockOpsContext) -> Result<u32, u32> {
    let op = ctx.op();

    // BPF_SOCK_OPS_PASSIVE_ESTABLISHED_CB = 4 (Server side of connection)
    // BPF_SOCK_OPS_ACTIVE_ESTABLISHED_CB = 5 (Client side of connection)
    if op != 4 && op != 5 {
        return Ok(0);
    }

    // Construct a key to identify this socket pair.
    // In a real scenario, this key needs to be carefully designed to match traffic direction.
    // Here we use a simplified 5-tuple key.
    let key = PacketLog {
        ipv4_src: ctx.remote_ip4(),
        ipv4_dst: ctx.local_ip4(),
        port_src: ctx.remote_port(),
        port_dst: ctx.local_port(),
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

    // Update the map: Key -> Socket
    // Use BPF_NOEXIST/BPF_ANY based on need.
    unsafe {
        SOCK_MAP.update(&key, &ctx, 0).map_err(|e| {
            info!(&ctx, "Failed to update SOCK_MAP: {}", e);
            1u32
        })?;
    }

    Ok(0)
}

/// 2. sk_msg: Redirect traffic
/// This program runs on sendmsg(). It looks up the peer socket in SOCK_MAP
/// and redirects data directly to its receive queue.
#[sk_msg]
pub fn bpf_redirect(ctx: SkMsgContext) -> u32 {
    match try_bpf_redirect(ctx) {
        Ok(ret) => ret,
        Err(ret) => ret,
    }
}

fn try_bpf_redirect(ctx: SkMsgContext) -> Result<u32, u32> {
    let msg = ctx.msg();

    // Construct the REVERSE key to find the peer socket.
    // If A connects to B:
    // A's local is src, B's remote is src.
    // To redirect A -> B, A looks for B's socket.
    //
    // Usually, you need two entries in the map or a smarter lookup.
    // For simplicity, let's assume we are redirecting "back to self" or a known peer logic.
    //
    // In a Sidecar (Envoy/Agw) scenario:
    // Traffic 1: App -> Sidecar (Localhost)
    // Traffic 2: Sidecar -> App (Localhost)

    // Let's rely on `msg_redirect_hash`.
    // It uses the Key to look up the socket in the Map.
    let key = PacketLog {
        ipv4_src: msg.remote_ip4(),
        ipv4_dst: msg.local_ip4(),
        port_src: msg.remote_port(),
        port_dst: msg.local_port(),
        len: 0,
    };

    // Redirect to the socket found in SOCK_MAP with `key`.
    // BPF_F_INGRESS flag puts it in the ingress queue of the target socket.
    match ctx.redirect_msg(
        &unsafe { SOCK_MAP },
        &key,
        aya_ebpf::bindings::BPF_F_INGRESS as u64,
    ) {
        Ok(_) => {
            // Check verdict
            return Ok(aya_ebpf::bindings::SK_PASS);
        }
        Err(_) => {
            // Fallback to normal stack if not found
            return Ok(aya_ebpf::bindings::SK_PASS);
        }
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    unsafe { core::hint::unreachable_unchecked() }
}
