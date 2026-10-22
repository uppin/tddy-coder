use super::{rust_module, tonic_service_module};

pub(super) fn compile_unbundled_families_1() -> Result<(), Box<dyn std::error::Error>> {
    // Host and worktree services — `#unbundle` node 1 split families E/F (hosts, telemetry, prompts,
    // keys) and G/H (worktrees, their disk usage, their files) out of `the pre-unbundle monolithic RPC coordinate`.
    //
    // Two separate passes rather than one over both files, because each service gets its own
    // generated module and its own `NAME`. No `.extern_path` is needed in either direction: the
    // closure of messages these 17 methods reach shares nothing with what stayed in
    // `connection.proto`, which is why the split needed no shared `types.proto`.
    {
        let proto = "proto/session_files.proto";
        prost_build::Config::new()
            .out_dir(std::env::var("OUT_DIR")?)
            .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
                generate_rpc_server: true,
                generate_tonic_adapter: false,
                rpc_crate_path: "tddy_rpc".to_string(),
                ..Default::default()
            }))
            .compile_protos(&[proto], &["proto"])?;
    }

    // Code navigation — definition, references and hover over a session's worktree, for the web's
    // code pane. Served by tddy-daemon, which forwards to the index daemon's `code_index` service;
    // no tonic pass, because nothing reaches it over the local gRPC socket.
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: false,
            rpc_crate_path: "tddy_rpc".to_string(),
            ..Default::default()
        }))
        .compile_protos(&["proto/code_navigation.proto"], &["proto"])?;
    Ok(())
}

pub(super) fn compile_unbundled_families_2() -> Result<(), Box<dyn std::error::Error>> {
    // Host and worktree — `#unbundle` node 1. Post-stack follow-up replaces hand-written adapters
    // in `tddy-session-lifecycle` with generated ones (same two-pass shape as node 7).
    for (proto, package) in [
        ("proto/host.proto", "host"),
        ("proto/worktree.proto", "worktree"),
    ] {
        prost_build::Config::new()
            .out_dir(std::env::var("OUT_DIR")?)
            .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
                generate_rpc_server: true,
                generate_tonic_adapter: true,
                rpc_crate_path: "tddy_rpc".to_string(),
                tonic_trait_path: Some(format!(
                    "crate::tonic_{package}::{}_service_server",
                    tonic_service_module(package)
                )),
            }))
            .compile_protos(&[proto], &["proto"])?;

        let dir = format!("{}/tonic_{package}", std::env::var("OUT_DIR")?);
        std::fs::create_dir_all(&dir)?;
        tonic_build::configure()
            .build_server(true)
            .build_client(true)
            .out_dir(&dir)
            .extern_path(format!(".{package}"), format!("crate::proto::{package}"))
            .compile_protos(&[proto], &["proto"])?;
    }
    Ok(())
}

pub(super) fn compile_unbundled_families_3() -> Result<(), Box<dyn std::error::Error>> {
    // `#unbundle` node 4 — family T (LiveKit rooms observability). Node 4 registered it on HTTP and
    // LiveKit but not on the local Unix socket; post-stack follow-up restores the socket mount with
    // the same two-pass shape as nodes 7–9.
    {
        let proto = "proto/livekit.proto";
        let package = "livekit";
        prost_build::Config::new()
            .out_dir(std::env::var("OUT_DIR")?)
            .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
                generate_rpc_server: true,
                generate_tonic_adapter: true,
                rpc_crate_path: "tddy_rpc".to_string(),
                tonic_trait_path: Some(format!(
                    "crate::proto::tonic_{package}::{}_service_server",
                    tonic_service_module(package)
                )),
            }))
            .compile_protos(&[proto], &["proto"])?;

        let dir = format!("{}/tonic_{package}", std::env::var("OUT_DIR")?);
        std::fs::create_dir_all(&dir)?;
        tonic_build::configure()
            .build_server(true)
            .build_client(true)
            .out_dir(&dir)
            .extern_path(
                format!(".{package}"),
                format!("crate::proto::{}", rust_module(package)),
            )
            .compile_protos(&[proto], &["proto"])?;
    }
    Ok(())
}

pub(super) fn compile_unbundled_families_4() -> Result<(), Box<dyn std::error::Error>> {
    // `#unbundle` node 7 — families B (the roster and conversations, five of whose methods are
    // named in the sandbox runner's relay allowlist) and M+N (activity, status, notifications and
    // ACP replay).
    //
    // Two passes each, not one, and outside the loop above: both services go on the daemon's local
    // Unix socket, because every family that was reachable there as part of
    // `the pre-unbundle monolithic RPC coordinate` stays reachable there after it moves — dropping one is a silent
    // capability removal on a privileged interface. So each gets the `generate_tonic_adapter` the
    // RpcService pass emits (17 adapter methods, generated rather than hand-written) plus the tonic
    // pass whose server trait that adapter implements. Same shape as `tddy-terminal-rpc`'s, and for
    // the same reason its comment gives: the two passes each emit a trait of the service's own name,
    // so the tonic one lands in its own module and the adapter reaches it through
    // `tonic_trait_path`.
    for (proto, package) in [
        ("proto/session_agents.proto", "session_agents"),
        ("proto/activity.proto", "activity"),
    ] {
        prost_build::Config::new()
            .out_dir(std::env::var("OUT_DIR")?)
            .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
                generate_rpc_server: true,
                generate_tonic_adapter: true,
                rpc_crate_path: "tddy_rpc".to_string(),
                tonic_trait_path: Some(format!(
                    "crate::proto::tonic_{package}::{}_service_server",
                    tonic_service_module(package)
                )),
            }))
            .compile_protos(&[proto], &["proto"])?;

        let dir = format!("{}/tonic_{package}", std::env::var("OUT_DIR")?);
        std::fs::create_dir_all(&dir)?;
        tonic_build::configure()
            .build_server(true)
            .build_client(true)
            .out_dir(&dir)
            // The service's own messages come back from the RpcService pass above, so this one emits
            // service code only and a request never crosses a re-encode between the two flavors.
            .extern_path(
                format!(".{package}"),
                format!("crate::proto::{}", rust_module(package)),
            )
            // `SessionAgentStatus` and `SessionAgentActivity` likewise: `ListSessions` reaches both
            // through `SessionEntry`, so they live in `types.proto` and there is one Rust type for
            // each however it arrives.
            .extern_path(".types", "crate::proto::types")
            .compile_protos(&[proto], &["proto"])?;
    }
    Ok(())
}

pub(super) fn compile_unbundled_families_5() -> Result<(), Box<dyn std::error::Error>> {
    // `#unbundle` node 8 — families A (the catalogue), L (tool execution) and P (PR-stack). Same
    // two-pass shape as node 7: each goes on the daemon's local Unix socket, so each gets
    // `generate_tonic_adapter` plus a tonic pass whose server trait that adapter implements.
    for (proto, package) in [
        ("proto/catalog.proto", "catalog"),
        ("proto/exec_tools.proto", "exec_tools"),
        ("proto/pr_stack.proto", "pr_stack"),
    ] {
        prost_build::Config::new()
            .out_dir(std::env::var("OUT_DIR")?)
            .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
                generate_rpc_server: true,
                generate_tonic_adapter: true,
                rpc_crate_path: "tddy_rpc".to_string(),
                tonic_trait_path: Some(format!(
                    "crate::proto::tonic_{package}::{}_service_server",
                    tonic_service_module(package)
                )),
            }))
            .compile_protos(&[proto], &["proto"])?;

        let dir = format!("{}/tonic_{package}", std::env::var("OUT_DIR")?);
        std::fs::create_dir_all(&dir)?;
        let mut tonic = tonic_build::configure()
            .build_server(true)
            .build_client(true)
            .out_dir(&dir)
            .extern_path(
                format!(".{package}"),
                format!("crate::proto::{}", rust_module(package)),
            );
        if package == "pr_stack" {
            tonic = tonic.extern_path(".types", "crate::proto::types");
        }
        tonic.compile_protos(&[proto], &["proto"])?;
    }
    Ok(())
}

pub(super) fn compile_unbundled_families_6() -> Result<(), Box<dyn std::error::Error>> {
    // `#unbundle` node 9 — families C, D, O and Q. Same two-pass shape as node 8: each goes on the
    // daemon's local Unix socket, so each gets `generate_tonic_adapter` plus a tonic pass.
    for (proto, package) in [
        ("proto/session.proto", "session"),
        ("proto/project.proto", "project"),
        ("proto/demo_vm.proto", "demo_vm"),
        ("proto/local_token.proto", "local_token"),
    ] {
        prost_build::Config::new()
            .out_dir(std::env::var("OUT_DIR")?)
            .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
                generate_rpc_server: true,
                generate_tonic_adapter: true,
                rpc_crate_path: "tddy_rpc".to_string(),
                tonic_trait_path: Some(format!(
                    "crate::proto::tonic_{package}::{}_service_server",
                    tonic_service_module(package)
                )),
            }))
            .compile_protos(&[proto], &["proto"])?;

        let dir = format!("{}/tonic_{package}", std::env::var("OUT_DIR")?);
        std::fs::create_dir_all(&dir)?;
        let mut tonic = tonic_build::configure()
            .build_server(true)
            .build_client(true)
            .out_dir(&dir)
            .extern_path(
                format!(".{package}"),
                format!("crate::proto::{}", rust_module(package)),
            );
        if package == "session" || package == "project" {
            tonic = tonic.extern_path(".types", "crate::proto::types");
        }
        tonic.compile_protos(&[proto], &["proto"])?;
    }
    Ok(())
}
