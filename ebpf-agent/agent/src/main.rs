use aya::programs::{SkMsg, SockOps};
use aya::{include_bytes_aligned, Bpf};
use aya_log::BpfLogger;
use clap::Parser;
use log::{info, warn};
use tokio::signal;

#[derive(Debug, Parser)]
struct Opt {
    #[clap(short, long, default_value = "cgroup_path")]
    cgroup: String,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    env_logger::init();
    let opt = Opt::parse();

    info!("Starting eBPF Agent...");

    // 1. Load the compiled BPF binary
    // Note: In real build, we need to compile `ebpf` crate first and point to it.
    // For now we assume a placeholder path or embedded bytes.
    let mut bpf = Bpf::load(include_bytes_aligned!(
        "../../target/bpfel-unknown-none/debug/ebpf-agent-ebpf"
    ))?;

    if let Err(e) = BpfLogger::init(&mut bpf) {
        warn!("failed to initialize eBPF logger: {}", e);
    }

    // 2. Load and Attach `sock_ops` program
    // This hooks into cgroup socket creation
    let program: &mut SockOps = bpf.program_mut("bpf_sockmap").unwrap().try_into()?;
    program.load()?;
    
    // Attach to cgroup v2 root (or specific container cgroup)
    // The cgroup path needs to be valid (e.g., /sys/fs/cgroup)
    let cgroup_file = std::fs::File::open(&opt.cgroup)?;
    program.attach(cgroup_file)?;
    info!("Attached sock_ops to cgroup: {}", opt.cgroup);

    // 3. Load and Attach `sk_msg` program
    // This hooks into the SOCK_MAP to handle redirection
    let sock_map = bpf.map_mut("SOCK_MAP").unwrap();
    let program_sk_msg: &mut SkMsg = bpf.program_mut("bpf_redirect").unwrap().try_into()?;
    program_sk_msg.load()?;
    program_sk_msg.attach(sock_map)?;
    info!("Attached sk_msg to SOCK_MAP");

    info!("eBPF Agent running (Sockmap Acceleration Active). Press Ctrl-C to exit.");
    signal::ctrl_c().await?;
    info!("Exiting...");

    Ok(())
}
