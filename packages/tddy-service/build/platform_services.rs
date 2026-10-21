pub(super) fn compile_platform_services_1() -> Result<(), Box<dyn std::error::Error>> {
    // Loopback TCP tunnel over LiveKit (bidi) — desktop proxy → session host 127.0.0.1:port
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: false,
            rpc_crate_path: "tddy_rpc".to_string(),
            ..Default::default()
        }))
        .compile_protos(&["proto/loopback_tunnel.proto"], &["proto"])?;

    // VM lifecycle service (async trait + RpcService server)
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: false,
            rpc_crate_path: "tddy_rpc".to_string(),
            ..Default::default()
        }))
        .compile_protos(&["proto/vm.proto"], &["proto"])?;

    // Background task management service (async trait + RpcService server)
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: false,
            rpc_crate_path: "tddy_rpc".to_string(),
            ..Default::default()
        }))
        .compile_protos(&["proto/tasks.proto"], &["proto"])?;

    // Model registry service (providers, models, assistants — the daemon's own SQLite registry)
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: false,
            rpc_crate_path: "tddy_rpc".to_string(),
            ..Default::default()
        }))
        .compile_protos(&["proto/models.proto"], &["proto"])?;

    // Unified action service (async trait + RpcService server)
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: false,
            rpc_crate_path: "tddy_rpc".to_string(),
            ..Default::default()
        }))
        .compile_protos(&["proto/actions.proto"], &["proto"])?;
    Ok(())
}

pub(super) fn compile_platform_services_2() -> Result<(), Box<dyn std::error::Error>> {
    // BSP build server service (async trait + RpcService server for /rpc + LiveKit)
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: false,
            rpc_crate_path: "tddy_rpc".to_string(),
            ..Default::default()
        }))
        .compile_protos(&["proto/bsp.proto"], &["proto"])?;

    // VNC control-plane service (VncService)
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: false,
            rpc_crate_path: "tddy_rpc".to_string(),
            ..Default::default()
        }))
        .compile_protos(&["proto/vnc.proto"], &["proto"])?;

    // VNC input forwarding service (VncInputService — bidi stream, served by tddy-vnc bridge)
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: false,
            rpc_crate_path: "tddy_rpc".to_string(),
            ..Default::default()
        }))
        .compile_protos(&["proto/vnc_input.proto"], &["proto"])?;

    // Screen sharing control-plane service (ScreenSharingService — VNC + RDP)
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: false,
            rpc_crate_path: "tddy_rpc".to_string(),
            ..Default::default()
        }))
        .compile_protos(&["proto/screen_sharing.proto"], &["proto"])?;

    // Screen sharing input forwarding service (bidi stream, served by protocol bridges)
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: false,
            rpc_crate_path: "tddy_rpc".to_string(),
            ..Default::default()
        }))
        .compile_protos(&["proto/screen_sharing_input.proto"], &["proto"])?;
    Ok(())
}

pub(super) fn compile_platform_services_3() -> Result<(), Box<dyn std::error::Error>> {
    // Sandbox service (darwin jail gRPC — served inside sandbox, daemon is client)
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: false,
            rpc_crate_path: "tddy_rpc".to_string(),
            ..Default::default()
        }))
        .compile_protos(&["proto/sandbox.proto"], &["proto"])?;
    Ok(())
}

fn sandbox_tonic_builder(tonic_sandbox_dir: &str) -> tonic_build::Builder {
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .out_dir(&tonic_sandbox_dir)
        .extern_path(
            ".exec_tools.ExecuteToolRequest",
            "crate::proto::exec_tools::ExecuteToolRequest",
        )
        .extern_path(
            ".exec_tools.ExecuteToolResponse",
            "crate::proto::exec_tools::ExecuteToolResponse",
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
}

pub(super) fn compile_platform_services_sandbox_tonic() -> Result<(), Box<dyn std::error::Error>> {
    let tonic_sandbox_dir = format!("{}/tonic_sandbox", std::env::var("OUT_DIR")?);
    std::fs::create_dir_all(&tonic_sandbox_dir)?;
    sandbox_tonic_builder(&tonic_sandbox_dir)
        .extern_path(
            ".sandbox.SandboxTerminalOutput",
            "crate::proto::sandbox::SandboxTerminalOutput",
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
    Ok(())
}
