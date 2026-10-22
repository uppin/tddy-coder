//! Code generation via tonic-build (remote.proto) and prost-build (echo, terminal).
//! Buf can be used for linting: `buf lint` from package directory.

// TODO(restructure-engine): `main` was split by hand into `build/*.rs` (consent D2): the engine reports
// `build.rs is not under packages/tddy-service/src, so it is no module of the package`.
#[path = "build/core_services.rs"]
mod core_services;
#[path = "build/descriptors.rs"]
mod descriptors;
#[path = "build/platform_services.rs"]
mod platform_services;
#[path = "build/unbundled_families.rs"]
mod unbundled_families;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    core_services::compile_core_services_1()?;
    core_services::compile_core_services_2()?;
    core_services::compile_core_services_3()?;
    core_services::compile_core_services_4()?;
    unbundled_families::compile_unbundled_families_1()?;
    unbundled_families::compile_unbundled_families_2()?;
    unbundled_families::compile_unbundled_families_3()?;
    unbundled_families::compile_unbundled_families_4()?;
    unbundled_families::compile_unbundled_families_5()?;
    unbundled_families::compile_unbundled_families_6()?;
    platform_services::compile_platform_services_1()?;
    platform_services::compile_platform_services_2()?;
    platform_services::compile_platform_services_3()?;
    platform_services::compile_platform_services_sandbox_tonic()?;
    descriptors::compile_descriptors_1()?;
    descriptors::compile_descriptors_2()?;
    descriptors::compile_descriptors_3()?;
    ensure_exec_tools_tonic_adapter()?;
    Ok(())
}

/// prost-build occasionally omits `ExecToolServiceTonicAdapter` for `exec_tools.proto`; append the
/// hand-maintained twin when missing so the local UDS surface stays complete.
fn ensure_exec_tools_tonic_adapter() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = std::env::var("OUT_DIR")?;
    let path = format!("{}/exec_tools.rs", out_dir);
    let body = std::fs::read_to_string(&path)?;
    if body.contains("ExecToolServiceTonicAdapter") {
        return Ok(());
    }
    let supplement = include_str!("exec_tool_tonic_adapter_supplement.rs");
    std::fs::write(path, format!("{body}\n{supplement}"))?;
    Ok(())
}

/// The `<service>_server` module tonic-build emits for the one service in each of node 7's protos.
///
/// Derived from the proto package rather than parsed out of the file: `session_agents.proto`
/// declares `SessionAgentService` and `activity.proto` declares `ActivityService`, so the module
/// names are `session_agent_service_server` and `activity_service_server`.
fn tonic_service_module(package: &str) -> &'static str {
    match package {
        "session_agents" => "session_agent",
        "activity" => "activity",
        "host" => "host",
        "worktree" => "worktree",
        "livekit" => "live_kit",
        "catalog" => "catalog",
        "exec_tools" => "exec_tool",
        "pr_stack" => "pr_stack",
        "session" => "session",
        "project" => "project",
        "demo_vm" => "demo_vm",
        "local_token" => "local_token",
        other => panic!("no tonic service module known for proto package {other}"),
    }
}

/// The `crate::proto` module each of node 7's protos is included under in `src/lib.rs`.
///
/// `session_agents.proto`'s prost output lands in `proto::session_agents_svc`, not
/// `proto::session_agents`: that name is already taken by the hand-written module holding the
/// `session.agents` broadcast topic and the relay allowlist.
fn rust_module(package: &str) -> &'static str {
    match package {
        "session_agents" => "session_agents_svc",
        "activity" => "activity",
        "livekit" => "livekit",
        "catalog" => "catalog",
        "exec_tools" => "exec_tools",
        "pr_stack" => "pr_stack",
        "session" => "session",
        "project" => "project",
        "demo_vm" => "demo_vm",
        "local_token" => "local_token",
        other => panic!("no Rust module known for proto package {other}"),
    }
}
