//! Compile envelope proto and echo service with LiveKitServiceGenerator.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // WebRTC (via livekit-ffi) bundles Objective-C categories on NSString.
    // Without -ObjC, the linker strips them, causing "unrecognized selector" at runtime.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "macos" || target_os == "ios" {
        println!("cargo:rustc-link-arg=-ObjC");
    }
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR")?);

    // Envelope proto (messages only)
    prost_build::Config::new()
        .out_dir(&out_dir)
        .compile_protos(&["proto/rpc_envelope.proto"], &["proto"])?;

    // Echo service with LiveKitServiceGenerator (async traits)
    prost_build::Config::new()
        .out_dir(&out_dir)
        .service_generator(Box::new(tddy_livekit_codegen::LiveKitServiceGenerator))
        .compile_protos(&["proto/test/echo_service.proto"], &["proto"])?;

    // Terminal service
    prost_build::Config::new()
        .out_dir(&out_dir)
        .service_generator(Box::new(tddy_livekit_codegen::LiveKitServiceGenerator))
        .compile_protos(&["proto/terminal.proto"], &["proto"])?;

    Ok(())
}
