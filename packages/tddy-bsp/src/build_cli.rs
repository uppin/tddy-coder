//! `build` / `build-list` subcommand dispatch.
//!
//! Local mode (default) runs `tddy-build` directly, through this crate's one
//! [`crate::plugins::plugin_registry`]. When `TDDY_SOCKET` is set the request is relayed to the
//! session-owning process, which serves it via the registered `tddy_core::BuildExecutor` (wired up
//! in `tddy-coder`).
//!
//! Moved here from `tddy-tools` by `#unbundle` node 5: the dispatch carried a second, verbatim copy
//! of the five-plugin registry, and keeping it in the CLI crate forced all six `tddy-build*`
//! dependencies on a crate that otherwise has no build knowledge at all. The relay reached the
//! socket through a `ToolcallRelay` function pointer until the same node moved the toolcall client
//! into `tddy-core`; it is [`tddy_core::toolcall::dispatch_toolcall`] directly now.

use anyhow::{Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_actions::tool::ProcessToolHost;
use tddy_build::executor::{BuildTools, ExecuteOptions};
use tddy_build::service::{build_json_with_tools, build_list_json, BuildListQuery};
use tddy_core::toolcall::dispatch_toolcall;
use tddy_daemon_sandbox::build_action_sandbox::DaemonActionSandbox;

use crate::plugins::plugin_registry;

/// Resolve `--repo-dir` to an absolute path. Action specs require absolute input
/// `host_path`s (they become jail mounts), so the repo root must be absolute before
/// it is joined with `srcs` patterns during lowering.
fn resolve_repo_dir(repo_dir: &Path) -> Result<PathBuf> {
    repo_dir
        .canonicalize()
        .with_context(|| format!("repo-dir not found: {}", repo_dir.display()))
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

    /// Run every action in a jail: its inputs read-only, its declared outputs the only writable
    /// paths, and only its tool closure executable. An action declaring `sandbox: true` runs that way
    /// without this flag.
    #[arg(long)]
    pub hermetic: bool,
}

pub async fn run_build_list(args: BuildListArgs) -> Result<()> {
    let repo_dir = resolve_repo_dir(&args.repo_dir)?;
    if let Some(socket_path) = std::env::var_os("TDDY_SOCKET") {
        return relay_build_list(Path::new(&socket_path), &args, &repo_dir).await;
    }
    let query = BuildListQuery {
        query: args.query.clone(),
        limit: args.limit,
        offset: args.offset,
    };
    let value = build_list_json(&repo_dir, &query).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("{}", serde_json::to_string(&value)?);
    Ok(())
}

pub async fn run_build(args: BuildArgs) -> Result<()> {
    let repo_dir = resolve_repo_dir(&args.repo_dir)?;
    if let Some(socket_path) = std::env::var_os("TDDY_SOCKET") {
        // The relayed build runs in the session's process, which does not take this flag: running
        // it there would build unconfined while the caller asked for a jail.
        if args.hermetic {
            anyhow::bail!(
                "--hermetic is not relayed to the session's build; run the build without TDDY_SOCKET"
            );
        }
        return relay_build(Path::new(&socket_path), &args, &repo_dir).await;
    }
    let registry = plugin_registry();
    let tools = local_build_tools()?;
    let opts = ExecuteOptions {
        no_cache: args.no_cache,
        dry_run: args.dry_run,
        hermetic: args.hermetic,
        ..ExecuteOptions::default()
    };
    let value = build_json_with_tools(
        &repo_dir,
        &args.target,
        &opts,
        tddy_build::BuildMode::Compile,
        &registry,
        &tools,
    )
    .await
    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("{}", serde_json::to_string(&value)?);
    Ok(())
}

/// The tools a local build resolves against: the built-ins with `<tddyhome>/tools` overrides, read
/// against this process. A `sandbox: true` action, or every action of a `--hermetic` run, is jailed
/// under `<tddyhome>/build-jails`.
fn local_build_tools() -> Result<BuildTools> {
    let home = tddy_home()?;
    let tools = BuildTools::load(&home.join("tools"), Arc::new(ProcessToolHost))
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    Ok(tools.with_sandbox(Arc::new(DaemonActionSandbox::new(home.join("build-jails")))))
}

/// The tddy home a local build reads from: `TDDY_DATA_DIR` when it is set, else the profile default,
/// else `$HOME/.tddy` — the rule the daemon resolves its data directory by. Absolute, because a
/// jail's directories must be.
fn tddy_home() -> Result<PathBuf> {
    let home = match std::env::var_os("TDDY_DATA_DIR").filter(|dir| !dir.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => match tddy_core::output::default_tddy_data_dir() {
            Some(dir) => dir,
            None => PathBuf::from(
                std::env::var_os("HOME").context("neither TDDY_DATA_DIR nor HOME is set")?,
            )
            .join(".tddy"),
        },
    };
    if home.is_absolute() {
        Ok(home)
    } else {
        Ok(std::env::current_dir()?.join(home))
    }
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
async fn relay_build_list(socket_path: &Path, args: &BuildListArgs, repo_dir: &Path) -> Result<()> {
    let request = BuildListRelayRequest {
        r#type: "build-list",
        repo_dir: repo_dir.to_string_lossy().into_owned(),
        query: &args.query,
        limit: args.limit,
        offset: args.offset,
    };
    relay(socket_path, &request).await
}

#[cfg(unix)]
async fn relay_build(socket_path: &Path, args: &BuildArgs, repo_dir: &Path) -> Result<()> {
    let request = BuildRelayRequest {
        r#type: "build",
        repo_dir: repo_dir.to_string_lossy().into_owned(),
        target: args.target.clone(),
        no_cache: args.no_cache,
        dry_run: args.dry_run,
    };
    relay(socket_path, &request).await
}

#[cfg(unix)]
async fn relay<T: Serialize>(socket_path: &Path, request: &T) -> Result<()> {
    let req = serde_json::to_value(request)?;
    let response_json = dispatch_toolcall(socket_path, req)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    let response: BuildRelayResponse =
        serde_json::from_value(response_json).with_context(|| "invalid response from relay")?;

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
async fn relay_build_list(
    _socket_path: &Path,
    _args: &BuildListArgs,
    _repo_dir: &Path,
) -> Result<()> {
    anyhow::bail!("TDDY_SOCKET relay is not supported on this platform")
}

#[cfg(not(unix))]
async fn relay_build(_socket_path: &Path, _args: &BuildArgs, _repo_dir: &Path) -> Result<()> {
    anyhow::bail!("TDDY_SOCKET relay is not supported on this platform")
}
