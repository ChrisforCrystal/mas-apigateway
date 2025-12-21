fn main() -> Result<(), Box<dyn std::error::Error>> {
    // We need to compile both protos or ensure compilation includes config.proto
    // However, tonic_build::compile_protos might only take one entry point.
    // If agw.proto imports config.proto, it should be fine IF the include path is set relevantly.
    // proto directory is not strictly the include path by default?
    // Let's use configure() to be safe about include paths.
    tonic_build::configure()
        .build_server(false)
        .build_client(true)
        .compile_protos(&["../proto/agw.proto"], &["../proto"])?;
    Ok(())
}
