//! Codegen for `supervisor.proto`.
//!
//! The privileged surface's types are generated here rather than in `tddy-service` so the only
//! process on the host that runs as uid 0 links a dependency tree small enough to audit: the
//! daemon's service catalog would otherwise pull a SQL engine, a TUI, an HTTP server and a TLS
//! stack into the privilege boundary, none of it reachable from this crate's code.
//!
//! RpcService flavor only, matching the pattern in `tddy-terminal-rpc/build.rs`: the surface rides
//! `tddy-rpc`'s frame codec over AF_UNIX and is deliberately absent from any gRPC descriptor set,
//! so reflection can never advertise it on a network-facing transport.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: false,
            rpc_crate_path: "tddy_rpc".to_string(),
        }))
        .compile_protos(&["proto/supervisor.proto"], &["proto"])?;

    Ok(())
}
