//! Compile envelope proto and echo service with LiveKitServiceGenerator.

fn main() -> Result<(), Box<dyn std::error::Error>> {
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

    Ok(())
}
