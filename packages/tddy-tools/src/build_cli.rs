//! `build` / `build-list` subcommands.
//!
//! Local mode (default) runs `tddy-build` directly. When `TDDY_SOCKET` is set the
//! request is relayed to the session-owning process, which serves it via the
//! registered `tddy_core::BuildExecutor` (wired up in `tddy-coder`).

use anyhow::{Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_build::plugin::PluginRegistry;
use tddy_build::service::{build_json, build_list_json, BuildListQuery};

/// Assemble the build-plugin registry from the recipe crates. This is the wiring
/// point: `tddy-build` knows no target types; the binary chooses the plugin set.
fn plugin_registry() -> PluginRegistry {
    let mut registry = PluginRegistry::new();
    registry.register(Arc::new(tddy_build_rust::RustPlugin));
    registry.register(Arc::new(tddy_build_typescript::TypeScriptPlugin));
    registry.register(Arc::new(tddy_build_docker::DockerPlugin));
    registry.register(Arc::new(tddy_build_buildroot::BuildrootPlugin));
    registry.register(Arc::new(tddy_build_qemu::QemuPlugin));
    registry
}

/// List build targets from `BUILD.yaml` manifests in a repository.
#[derive(Parser)]
#[command(name = "build-list")]
pub struct BuildListArgs {
    /// Repository root to discover manifests in.
    #[arg(long)]
    pub repo_dir: PathBuf,

    /// Case-insensitive substring filter on target id, name, or type.
    #[arg(long)]
    pub query: Option<String>,

    /// Maximum targets to return.
    #[arg(long)]
    pub limit: Option<usize>,

    /// Zero-based offset into the result set (for pagination).
    #[arg(long, default_value_t = 0)]
    pub offset: usize,
}

/// Build a target.
#[derive(Parser)]
#[command(name = "build")]
pub struct BuildArgs {
    /// Repository root to discover manifests in.
    #[arg(long)]
    pub repo_dir: PathBuf,

    /// Target id to build.
    #[arg(long)]
    pub target: String,

    /// Bypass the action cache (read and write).
    #[arg(long)]
    pub no_cache: bool,

    /// Print the planned argv per action without executing.
    #[arg(long)]
    pub dry_run: bool,
}

pub fn run_build_list(args: BuildListArgs) -> Result<()> {
    if let Some(socket_path) = std::env::var_os("TDDY_SOCKET") {
        return relay_build_list(Path::new(&socket_path), &args);
    }
    let query = BuildListQuery {
        query: args.query.clone(),
        limit: args.limit,
        offset: args.offset,
    };
    let value =
        build_list_json(&args.repo_dir, &query).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("{}", serde_json::to_string(&value)?);
    Ok(())
}

pub async fn run_build(args: BuildArgs) -> Result<()> {
    if let Some(socket_path) = std::env::var_os("TDDY_SOCKET") {
        return relay_build(Path::new(&socket_path), &args);
    }
    let registry = plugin_registry();
    let value = build_json(
        &args.repo_dir,
        &args.target,
        args.no_cache,
        args.dry_run,
        &registry,
    )
    .await
    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("{}", serde_json::to_string(&value)?);
    Ok(())
}

#[derive(Debug, Serialize)]
struct BuildListRelayRequest<'a> {
    r#type: &'static str,
    repo_dir: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    query: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<usize>,
    offset: usize,
}

#[derive(Debug, Serialize)]
struct BuildRelayRequest {
    r#type: &'static str,
    repo_dir: String,
    target: String,
    no_cache: bool,
    dry_run: bool,
}

/// Relay response: an opaque JSON object (`status` + the executor's payload).
#[derive(Debug, Deserialize)]
struct BuildRelayResponse {
    #[serde(default)]
    status: String,
    #[serde(default)]
    message: Option<String>,
    #[serde(flatten)]
    rest: serde_json::Map<String, serde_json::Value>,
}

#[cfg(unix)]
fn relay_build_list(socket_path: &Path, args: &BuildListArgs) -> Result<()> {
    let request = BuildListRelayRequest {
        r#type: "build-list",
        repo_dir: args.repo_dir.to_string_lossy().into_owned(),
        query: &args.query,
        limit: args.limit,
        offset: args.offset,
    };
    relay(socket_path, &request)
}

#[cfg(unix)]
fn relay_build(socket_path: &Path, args: &BuildArgs) -> Result<()> {
    let request = BuildRelayRequest {
        r#type: "build",
        repo_dir: args.repo_dir.to_string_lossy().into_owned(),
        target: args.target.clone(),
        no_cache: args.no_cache,
        dry_run: args.dry_run,
    };
    relay(socket_path, &request)
}

#[cfg(unix)]
fn relay<T: Serialize>(socket_path: &Path, request: &T) -> Result<()> {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;

    let mut stream = UnixStream::connect(socket_path).with_context(|| {
        format!(
            "failed to connect to TDDY_SOCKET: {}",
            socket_path.display()
        )
    })?;
    let line = serde_json::to_string(request)?;
    stream.write_all(line.as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()?;

    let mut reader = BufReader::new(&mut stream);
    let mut response_line = String::new();
    reader.read_line(&mut response_line)?;
    let response_line = response_line.trim();

    let response: BuildRelayResponse = serde_json::from_str(response_line)
        .with_context(|| format!("invalid response from relay: {}", response_line))?;

    if response.status == "error" {
        let msg = response
            .message
            .as_deref()
            .unwrap_or("build relay failed")
            .to_string();
        eprintln!("{msg}");
        println!("{}", serde_json::json!({"status":"error","message":msg}));
        std::process::exit(1);
    }

    // Re-emit the executor payload verbatim (status + targets/record fields).
    let mut object = response.rest;
    object.insert(
        "status".to_string(),
        serde_json::Value::String(if response.status.is_empty() {
            "ok".to_string()
        } else {
            response.status
        }),
    );
    println!(
        "{}",
        serde_json::to_string(&serde_json::Value::Object(object))?
    );
    Ok(())
}

#[cfg(not(unix))]
fn relay_build_list(_socket_path: &Path, _args: &BuildListArgs) -> Result<()> {
    anyhow::bail!("TDDY_SOCKET relay is not supported on this platform")
}

#[cfg(not(unix))]
fn relay_build(_socket_path: &Path, _args: &BuildArgs) -> Result<()> {
    anyhow::bail!("TDDY_SOCKET relay is not supported on this platform")
}
