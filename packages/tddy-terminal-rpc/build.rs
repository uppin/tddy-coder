//! Codegen for `terminal_session.proto`:
//! - a prost pass with the `tddy-rpc` `RpcService` server trait (for LiveKit / stdio transports)
//!   plus the tonic adapter that delegates one implementation of that trait to the tonic server
//!   trait emitted below;
//! - a tonic pass (gRPC / Connect-HTTP server + client) reusing the canonical prost message types
//!   via `extern_path`, mirroring the pattern in `tddy-service/build.rs`.
//!
//! The two passes each emit a `TerminalSessionService` trait, so the tonic one lands in its own
//! module (`proto::tonic_terminal_session`) and the adapter reaches it through the
//! `tonic_trait_path` below rather than an unqualified import.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // RpcService-flavored pass: async trait + RpcService server for LiveKit/tddy-rpc.
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: true,
            rpc_crate_path: "tddy_rpc".to_string(),
            tonic_trait_path: Some(
                "crate::proto::tonic_terminal_session::terminal_session_service_server".to_string(),
            ),
        }))
        .compile_protos(&["proto/terminal_session.proto"], &["proto"])?;

    // Tonic gRPC server/client, reusing the prost message types above so both `TerminalSessionService`
    // trait impls (tonic and RpcService) operate on identical Rust types. `src/lib.rs` includes
    // this output — without that include the adapter's `tonic_trait_path` would not resolve and the
    // gRPC server trait would be generated but unreachable.
    let tonic_dir = format!("{}/tonic_terminal_session", std::env::var("OUT_DIR")?);
    std::fs::create_dir_all(&tonic_dir)?;
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .out_dir(&tonic_dir)
        .extern_path(".terminal_session", "crate::proto::terminal_session")
        .compile_protos(&["proto/terminal_session.proto"], &["proto"])?;

    Ok(())
}
