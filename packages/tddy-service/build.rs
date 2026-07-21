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

    // TddyRemote as an RpcService (async trait + RpcService server for stdio and LiveKit/tddy-rpc),
    // reusing the tonic-generated message types above via extern_path — one canonical Rust type
    // per message, no re-encode/decode bridging needed between transports. Own out_dir
    // subdirectory: this pass's default filename (`tddy.v1.rs`, from the proto package) would
    // otherwise collide with the first tonic_build pass's own `tddy.v1.rs` in the top-level
    // OUT_DIR.
    let rpc_remote_dir = format!("{}/rpc_remote", std::env::var("OUT_DIR")?);
    std::fs::create_dir_all(&rpc_remote_dir)?;
    prost_build::Config::new()
        .out_dir(&rpc_remote_dir)
        .extern_path(".tddy.v1.ClientMessage", "crate::gen::ClientMessage")
        .extern_path(".tddy.v1.ServerMessage", "crate::gen::ServerMessage")
        .extern_path(
            ".tddy.v1.GetSessionRequest",
            "crate::gen::GetSessionRequest",
        )
        .extern_path(
            ".tddy.v1.GetSessionResponse",
            "crate::gen::GetSessionResponse",
        )
        .extern_path(
            ".tddy.v1.ListSessionsRequest",
            "crate::gen::ListSessionsRequest",
        )
        .extern_path(
            ".tddy.v1.ListSessionsResponse",
            "crate::gen::ListSessionsResponse",
        )
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: false,
            rpc_crate_path: "tddy_rpc".to_string(),
        }))
        .compile_protos(&["proto/tddy/v1/remote.proto"], &["proto"])?;

    // AcpService: protobuf mirror of ACP (async trait + RpcService server for stdio and
    // LiveKit/tddy-rpc). Self-contained proto — no extern_path needed. Output file name derives
    // from the proto package (`tddy.acp.v1` -> `tddy.acp.v1.rs`), distinct from the tonic pass's
    // `tddy.v1.rs`, so no OUT_DIR collision.
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: false,
            rpc_crate_path: "tddy_rpc".to_string(),
        }))
        .compile_protos(&["proto/tddy/acp/v1/acp.proto"], &["proto"])?;

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

    // Connection service (tonic gRPC server/client, reusing the canonical prost message types).
    // `.extern_path(".connection", ...)` remaps every `.connection.*` message to the structs
    // generated by the tddy-rpc pass above, so this pass emits only service code (server trait,
    // `ConnectionServiceServer<T>`, `ConnectionServiceClient`) referencing those canonical types —
    // no duplicate message structs.
    let tonic_connection_dir = format!("{}/tonic_connection", std::env::var("OUT_DIR")?);
    std::fs::create_dir_all(&tonic_connection_dir)?;
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .out_dir(&tonic_connection_dir)
        .extern_path(".connection", "crate::proto::connection")
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
        // sandbox.proto's own message types: reuse the RpcService-flavored pass's canonical
        // structs above (same pattern as terminal.proto) so both `SandboxService` trait impls
        // (tonic and RpcService/stdio) operate on identical Rust types.
        .extern_path(
            ".sandbox.SessionFrame",
            "crate::proto::sandbox::SessionFrame",
        )
        .extern_path(".sandbox.HostPoll", "crate::proto::sandbox::HostPoll")
        .extern_path(
            ".sandbox.SubscribeTerminal",
            "crate::proto::sandbox::SubscribeTerminal",
        )
        .extern_path(
            ".sandbox.SandboxInput",
            "crate::proto::sandbox::SandboxInput",
        )
        .extern_path(".sandbox.EchoRequest", "crate::proto::sandbox::EchoRequest")
        .extern_path(
            ".sandbox.EchoResponse",
            "crate::proto::sandbox::EchoResponse",
        )
        .extern_path(
            ".sandbox.EchoStreamFrame",
            "crate::proto::sandbox::EchoStreamFrame",
        )
        .extern_path(
            ".sandbox.EgressRequest",
            "crate::proto::sandbox::EgressRequest",
        )
        .extern_path(
            ".sandbox.EgressResponse",
            "crate::proto::sandbox::EgressResponse",
        )
        .extern_path(
            ".sandbox.EgressHeader",
            "crate::proto::sandbox::EgressHeader",
        )
        .extern_path(".sandbox.TunnelOpen", "crate::proto::sandbox::TunnelOpen")
        .extern_path(
            ".sandbox.TunnelOpenAck",
            "crate::proto::sandbox::TunnelOpenAck",
        )
        .extern_path(".sandbox.TunnelData", "crate::proto::sandbox::TunnelData")
        .extern_path(".sandbox.TunnelClose", "crate::proto::sandbox::TunnelClose")
        .extern_path(
            ".sandbox.SessionEnded",
            "crate::proto::sandbox::SessionEnded",
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
