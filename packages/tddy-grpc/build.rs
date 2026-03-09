//! Code generation via tonic-build (standard Rust approach).
//! Buf can be used for linting: `buf lint` from package directory.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&["proto/tddy/v1/remote.proto"], &["proto"])?;
    Ok(())
}
