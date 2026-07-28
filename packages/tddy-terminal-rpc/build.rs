//! Codegen for `terminal_session.proto`:
//! - a prost pass with the `tddy-rpc` `RpcService` server trait (for LiveKit / stdio transports);
//! - a tonic pass (gRPC / Connect-HTTP server + client) reusing the canonical prost message types
//!   via `extern_path`, mirroring the pattern in `tddy-service/build.rs`.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // RpcService-flavored pass: async trait + RpcService server for LiveKit/tddy-rpc.
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: false,
            rpc_crate_path: "tddy_rpc".to_string(),
        }))
        .compile_protos(&["proto/terminal_session.proto"], &["proto"])?;

    // Tonic gRPC server/client, reusing the prost message types above so both `TerminalSessionService`
    // trait impls (tonic and RpcService) operate on identical Rust types.
    let tonic_dir = format!("{}/tonic_terminal_session", std::env::var("OUT_DIR")?);
    std::fs::create_dir_all(&tonic_dir)?;
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .out_dir(&tonic_dir)
        .extern_path(
            ".terminal_session",
            "crate::proto::terminal_session",
        )
        .compile_protos(&["proto/terminal_session.proto"], &["proto"])?;

    Ok(())
}
