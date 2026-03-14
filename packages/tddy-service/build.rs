//! Code generation via tonic-build (remote.proto) and prost-build (echo, terminal).
//! Buf can be used for linting: `buf lint` from package directory.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // TddyRemote (gRPC)
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&["proto/tddy/v1/remote.proto"], &["proto"])?;

    // Echo service (async trait + RpcService server for LiveKit/tddy-rpc)
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: true,
            rpc_crate_path: "tddy_rpc".to_string(),
        }))
        .compile_protos(&["proto/test/echo_service.proto"], &["proto"])?;

    // Terminal service
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: false,
            rpc_crate_path: "tddy_rpc".to_string(),
        }))
        .compile_protos(&["proto/terminal.proto"], &["proto"])?;

    // Token service (async trait + RpcService server + tonic adapter)
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: true,
            rpc_crate_path: "tddy_rpc".to_string(),
        }))
        .compile_protos(&["proto/token.proto"], &["proto"])?;

    // GitHub Auth service (async trait + RpcService server)
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: false,
            rpc_crate_path: "tddy_rpc".to_string(),
        }))
        .compile_protos(&["proto/github_auth.proto"], &["proto"])?;

    Ok(())
}
