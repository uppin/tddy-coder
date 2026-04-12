//! Code generation via tonic-build (remote.proto) and prost-build (echo, terminal).
//! Buf can be used for linting: `buf lint` from package directory.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // TddyRemote (gRPC)
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(
            &[
                "proto/tddy/v1/remote.proto",
                "proto/tddy/v1/observer.proto",
                "proto/tddy/v1/presenter_intent.proto",
            ],
            &["proto"],
        )?;

    // Echo service (async trait + RpcService server for LiveKit/tddy-rpc)
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: true,
            rpc_crate_path: "tddy_rpc".to_string(),
        }))
        .compile_protos(&["proto/test/echo_service.proto"], &["proto"])?;

    // Terminal service (async trait + RpcService server for LiveKit/tddy-rpc)
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: false,
            rpc_crate_path: "tddy_rpc".to_string(),
        }))
        .compile_protos(&["proto/terminal.proto"], &["proto"])?;

    // Terminal service (tonic gRPC server/client, reusing prost message types)
    let tonic_terminal_dir = format!("{}/tonic_terminal", std::env::var("OUT_DIR")?);
    std::fs::create_dir_all(&tonic_terminal_dir)?;
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .out_dir(&tonic_terminal_dir)
        .extern_path(
            ".terminal.TerminalInput",
            "crate::proto::terminal::TerminalInput",
        )
        .extern_path(
            ".terminal.TerminalOutput",
            "crate::proto::terminal::TerminalOutput",
        )
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

    // Auth service (async trait + RpcService server)
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: false,
            rpc_crate_path: "tddy_rpc".to_string(),
        }))
        .compile_protos(&["proto/auth.proto"], &["proto"])?;

    // Connection service (daemon session/tool management)
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: false,
            rpc_crate_path: "tddy_rpc".to_string(),
        }))
        .compile_protos(&["proto/connection.proto"], &["proto"])?;

    // Loopback TCP tunnel over LiveKit (bidi) — desktop proxy → session host 127.0.0.1:port
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: false,
            rpc_crate_path: "tddy_rpc".to_string(),
        }))
        .compile_protos(&["proto/loopback_tunnel.proto"], &["proto"])?;

    // Tunnel management (daemon Connect /rpc): advertisements, start/stop, browser open.
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: false,
            rpc_crate_path: "tddy_rpc".to_string(),
        }))
        .compile_protos(&["proto/tunnel_management.proto"], &["proto"])?;

    Ok(())
}
