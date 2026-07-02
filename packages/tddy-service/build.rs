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

    // TddyRemote (async trait + RpcService server for LiveKit/tddy-rpc). The plain-gRPC path
    // above still owns the tonic server/client for remote.proto; this second pass exists so
    // TddyRemoteService can *also* be registered in a LiveKit MultiRpcService — previously it
    // only implemented the tonic trait and could never be wrapped as an RpcService.
    // Dedicated out_dir: the tonic_build call above also compiles package `tddy.v1` (shared
    // with observer.proto/presenter_intent.proto) into the default OUT_DIR as `tddy.v1.rs` —
    // without a separate directory here, prost_build's own `tddy.v1.rs` output for this pass
    // would silently overwrite (or be overwritten by) that file, since both default to the same
    // package-derived filename in the same OUT_DIR.
    let tddy_remote_rpc_dir = format!("{}/tddy_remote_rpc", std::env::var("OUT_DIR")?);
    std::fs::create_dir_all(&tddy_remote_rpc_dir)?;
    prost_build::Config::new()
        .out_dir(&tddy_remote_rpc_dir)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: false,
            rpc_crate_path: "tddy_rpc".to_string(),
        }))
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

    // VM lifecycle service (async trait + RpcService server)
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: false,
            rpc_crate_path: "tddy_rpc".to_string(),
        }))
        .compile_protos(&["proto/vm.proto"], &["proto"])?;

    // Background task management service (async trait + RpcService server)
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: false,
            rpc_crate_path: "tddy_rpc".to_string(),
        }))
        .compile_protos(&["proto/tasks.proto"], &["proto"])?;

    // Unified action service (async trait + RpcService server)
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: false,
            rpc_crate_path: "tddy_rpc".to_string(),
        }))
        .compile_protos(&["proto/actions.proto"], &["proto"])?;

    // VNC control-plane service (VncService)
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: false,
            rpc_crate_path: "tddy_rpc".to_string(),
        }))
        .compile_protos(&["proto/vnc.proto"], &["proto"])?;

    // VNC input forwarding service (VncInputService — bidi stream, served by tddy-vnc bridge)
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: false,
            rpc_crate_path: "tddy_rpc".to_string(),
        }))
        .compile_protos(&["proto/vnc_input.proto"], &["proto"])?;

    // Screen sharing control-plane service (ScreenSharingService — VNC + RDP)
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: false,
            rpc_crate_path: "tddy_rpc".to_string(),
        }))
        .compile_protos(&["proto/screen_sharing.proto"], &["proto"])?;

    // Screen sharing input forwarding service (bidi stream, served by protocol bridges)
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: false,
            rpc_crate_path: "tddy_rpc".to_string(),
        }))
        .compile_protos(&["proto/screen_sharing_input.proto"], &["proto"])?;

    // Sandbox service (darwin jail gRPC — served inside sandbox, daemon is client)
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: false,
            rpc_crate_path: "tddy_rpc".to_string(),
        }))
        .compile_protos(&["proto/sandbox.proto"], &["proto"])?;

    let tonic_sandbox_dir = format!("{}/tonic_sandbox", std::env::var("OUT_DIR")?);
    std::fs::create_dir_all(&tonic_sandbox_dir)?;
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .out_dir(&tonic_sandbox_dir)
        .extern_path(
            ".connection.SessionTerminalOutput",
            "crate::proto::connection::SessionTerminalOutput",
        )
        .extern_path(
            ".connection.ExecuteToolRequest",
            "crate::proto::connection::ExecuteToolRequest",
        )
        .extern_path(
            ".connection.ExecuteToolResponse",
            "crate::proto::connection::ExecuteToolResponse",
        )
        .compile_protos(&["proto/sandbox.proto"], &["proto"])?;

    // Descriptor-only pass: emit a combined FileDescriptorSet for ALL service protos.
    // The reflection service reads this at runtime to serve descriptors.
    let descriptor_path = format!("{}/service_descriptors.bin", std::env::var("OUT_DIR")?);
    let descriptor_scratch_dir = format!("{}/descriptor_set_only", std::env::var("OUT_DIR")?);
    std::fs::create_dir_all(&descriptor_scratch_dir)?;
    prost_build::Config::new()
        .file_descriptor_set_path(&descriptor_path)
        .out_dir(&descriptor_scratch_dir)
        .compile_protos(
            &[
                "proto/test/echo_service.proto",
                "proto/terminal.proto",
                "proto/token.proto",
                "proto/auth.proto",
                "proto/connection.proto",
                "proto/loopback_tunnel.proto",
                "proto/vm.proto",
                "proto/tasks.proto",
                "proto/actions.proto",
                "proto/vnc.proto",
                "proto/vnc_input.proto",
                "proto/screen_sharing.proto",
                "proto/screen_sharing_input.proto",
                "proto/sandbox.proto",
                "proto/grpc/reflection/v1/reflection.proto",
            ],
            &["proto"],
        )?;

    // Server reflection service (async trait + RpcService server for LiveKit/tddy-rpc)
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: false,
            rpc_crate_path: "tddy_rpc".to_string(),
        }))
        .compile_protos(&["proto/grpc/reflection/v1/reflection.proto"], &["proto"])?;

    Ok(())
}
