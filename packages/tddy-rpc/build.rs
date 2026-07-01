//! Compile the wire envelope proto shared by every RPC transport.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR")?);

    prost_build::Config::new()
        .out_dir(&out_dir)
        .compile_protos(&["proto/rpc_envelope.proto"], &["proto"])?;

    Ok(())
}
