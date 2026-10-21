pub(super) fn compile_core_services_1() -> Result<(), Box<dyn std::error::Error>> {
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
            ..Default::default()
        }))
        .compile_protos(&["proto/tddy/v1/remote.proto"], &["proto"])?;
    Ok(())
}

pub(super) fn compile_core_services_2() -> Result<(), Box<dyn std::error::Error>> {
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
            ..Default::default()
        }))
        .compile_protos(&["proto/tddy/acp/v1/acp.proto"], &["proto"])?;

    // Echo service (async trait + RpcService server for LiveKit/tddy-rpc)
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: true,
            rpc_crate_path: "tddy_rpc".to_string(),
            ..Default::default()
        }))
        .compile_protos(&["proto/test/echo_service.proto"], &["proto"])?;

    // Terminal service (async trait + RpcService server for LiveKit/tddy-rpc)
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: false,
            rpc_crate_path: "tddy_rpc".to_string(),
            ..Default::default()
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
    Ok(())
}

pub(super) fn compile_core_services_3() -> Result<(), Box<dyn std::error::Error>> {
    // Remote git service (async trait + RpcService server for LiveKit/tddy-rpc). Bidi-only: one
    // git pack operation per stream. No tonic pass — git never reaches the daemon over gRPC.
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: false,
            rpc_crate_path: "tddy_rpc".to_string(),
            ..Default::default()
        }))
        .compile_protos(&["proto/remote_git.proto"], &["proto"])?;

    // Session admission service (async trait + RpcService server for LiveKit/tddy-rpc). The
    // facilitating daemon admits an owning daemon to a session room with a scoped short-TTL
    // token (PRD § "What attach does" step 3 — the room-admission handshake). Served on the
    // common-room `daemon-{A}` participant so an owning daemon not yet in the session room can
    // still reach A. No tonic pass — admission is never served over gRPC.
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: false,
            rpc_crate_path: "tddy_rpc".to_string(),
            ..Default::default()
        }))
        .compile_protos(&["proto/session_admission.proto"], &["proto"])?;

    // Token service (async trait + RpcService server + tonic adapter)
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: true,
            rpc_crate_path: "tddy_rpc".to_string(),
            ..Default::default()
        }))
        .compile_protos(&["proto/token.proto"], &["proto"])?;

    // Auth service (async trait + RpcService server). The two requests carrying a vault passphrase
    // get no derived `Debug` — it would print the passphrase — and format it redacted instead
    // (`src/auth_redacted_debug.rs`).
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .skip_debug([".auth.UnlockVaultRequest", ".auth.ResetVaultRequest"])
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: false,
            rpc_crate_path: "tddy_rpc".to_string(),
            ..Default::default()
        }))
        .compile_protos(&["proto/auth.proto"], &["proto"])?;
    Ok(())
}

pub(super) fn compile_core_services_4() -> Result<(), Box<dyn std::error::Error>> {
    // Accounts service (`#keyring` 4/9) — async trait + RpcService server, and no tonic pass.
    // `accounts.AccountsService` is asked by a browser over whatever wire the host connection was
    // opened on; nothing addresses it over the daemon's local Unix socket, so it needs no tonic
    // adapter. Same single-pass shape as `auth.proto` above, for the same reason.
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: false,
            rpc_crate_path: "tddy_rpc".to_string(),
            ..Default::default()
        }))
        .compile_protos(&["proto/accounts.proto"], &["proto"])?;

    // Askpass wire: messages only. The loopback endpoint in `tddy-host-service::askpass` is a
    // hand-written axum route at the Connect path, so no service code is generated.
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .compile_protos(&["proto/askpass.proto"], &["proto"])?;
    Ok(())
}
