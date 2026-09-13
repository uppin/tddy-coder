//! `restructure` subcommands: apply/check/status/anchors/verify JSONL plans.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tddy_lsp::allowlist::{Language, LaunchSpec, LspAllowList};
use tddy_lsp::registry::{LspKey, LspRegistry};
use tddy_task::TaskRegistry;

#[derive(Parser)]
#[command(name = "restructure")]
pub struct RestructureArgs {
    #[command(subcommand)]
    pub command: RestructureCommand,
}

#[derive(Subcommand)]
pub enum RestructureCommand {
    /// Execute a JSONL refactoring plan (default).
    Apply(RestructurePlanArgs),
    /// Count journal statuses for a plan.
    Status(RestructurePlanArgs),
    /// Static (+ optional deep) preflight without writes.
    Check(RestructureCheckArgs),
    /// Emit a range anchor covering named items.
    Anchors(RestructureAnchorsArgs),
    /// Compare statement multisets against a git ref.
    Verify(RestructureVerifyArgs),
}

#[derive(Parser)]
pub struct RestructurePlanArgs {
    /// Path to the plan JSONL file.
    pub plan: PathBuf,

    #[arg(long)]
    pub dry_run: bool,

    #[arg(long)]
    pub resume: bool,

    #[arg(long)]
    pub from: Option<usize>,

    #[arg(long)]
    pub stop_after: Option<usize>,

    #[arg(long)]
    pub indexing_budget: Option<u64>,
}

#[derive(Parser)]
pub struct RestructureCheckArgs {
    pub plan: PathBuf,

    /// Also resolve every operation through rust-analyzer, reporting the refusals an apply would
    /// give and the blast radius of every cross-crate move.
    #[arg(long)]
    pub deep: bool,

    /// Report every file the plan names that is longer than this many lines.
    #[arg(long)]
    pub budget: Option<usize>,

    #[arg(long)]
    pub indexing_budget: Option<u64>,
}

#[derive(Parser)]
pub struct RestructureAnchorsArgs {
    pub file: PathBuf,

    #[arg(long, value_delimiter = ',')]
    pub items: Vec<String>,

    #[arg(long)]
    pub indexing_budget: Option<u64>,
}

#[derive(Parser)]
pub struct RestructureVerifyArgs {
    #[arg(long)]
    pub against: String,
}

pub async fn run(args: RestructureArgs) -> Result<()> {
    let cli_args = cli_vector(args);
    let needs_lsp = needs_lsp_client(&cli_args);

    let client = if needs_lsp {
        let root = std::env::current_dir().context("current_dir")?;
        let task_registry = TaskRegistry::new();
        let lsp_registry = LspRegistry::new(
            restructure_allow_list(),
            task_registry,
            Duration::from_secs(600),
        );
        let key = LspKey {
            root,
            language: Language::Rust,
        };
        let service = lsp_registry
            .get_or_spawn(key)
            .await
            .context("rust-analyzer LSP")?;
        // The client's own per-request default is sized for interactive queries. A code-action
        // request against a cold index routinely outlasts it, and `--indexing-budget` is
        // documented as the remedy — so it has to reach the wait that actually fires.
        service
            .client
            .set_request_timeout(request_timeout(&cli_args));
        Some(Arc::clone(&service.client))
    } else {
        None
    };

    tokio::task::spawn_blocking(move || tddy_code_restructuring::runner::run(&cli_args, client))
        .await
        .context("restructure task join")?
        .map_err(anyhow::Error::msg)
}

/// rust-analyzer, launched with the handshake the restructure backend needs.
///
/// `LspAllowList::rust_only` advertises nothing, and a server told nothing answers accordingly:
/// it returns no code actions at all — which reads as a range that supports no refactoring —
/// and it counts positions in utf-16 code units while this client counts bytes. Both are
/// settled by the handshake, so the handshake is what this carries.
fn restructure_allow_list() -> LspAllowList {
    let mut allow = LspAllowList::new();
    allow.allow(
        Language::Rust,
        LaunchSpec::new("rust-analyzer")
            .with_capabilities(tddy_code_restructuring::client_capabilities())
            .with_initialization_options(tddy_code_restructuring::server_settings()),
    );
    allow
}

/// How long one LSP request may take, taken from `--indexing-budget` where it was given.
///
/// The budget is the caller's statement of how long the whole resolution may take, so no single
/// request inside it should be cut short by a smaller default.
fn request_timeout(args: &[String]) -> Duration {
    let budget = args
        .iter()
        .position(|a| a == "--indexing-budget")
        .and_then(|at| args.get(at + 1))
        .and_then(|seconds| seconds.parse::<u64>().ok());
    Duration::from_secs(budget.unwrap_or(DEFAULT_INDEXING_BUDGET_SECONDS))
}

/// The backend's own warm-up budget, used when `--indexing-budget` is absent so that the
/// request wait and the retry loop expire together.
const DEFAULT_INDEXING_BUDGET_SECONDS: u64 = 600;

fn needs_lsp_client(args: &[String]) -> bool {
    match args.first().map(String::as_str) {
        Some("apply") | Some("anchors") => true,
        Some("check") => args.iter().any(|a| a == "--deep"),
        _ => false,
    }
}

fn cli_vector(args: RestructureArgs) -> Vec<String> {
    match args.command {
        RestructureCommand::Apply(plan) => {
            let mut v = vec!["apply".to_string(), plan.plan.display().to_string()];
            push_flag(&mut v, "--dry-run", plan.dry_run);
            push_flag(&mut v, "--resume", plan.resume);
            push_opt(&mut v, "--from", plan.from);
            push_opt(&mut v, "--stop-after", plan.stop_after);
            push_opt(&mut v, "--indexing-budget", plan.indexing_budget);
            v
        }
        RestructureCommand::Status(plan) => {
            vec!["status".to_string(), plan.plan.display().to_string()]
        }
        RestructureCommand::Check(check) => {
            let mut v = vec!["check".to_string(), check.plan.display().to_string()];
            push_flag(&mut v, "--deep", check.deep);
            push_opt(&mut v, "--budget", check.budget);
            push_opt(&mut v, "--indexing-budget", check.indexing_budget);
            v
        }
        RestructureCommand::Anchors(anchors) => {
            let mut v = vec![
                "anchors".to_string(),
                anchors.file.display().to_string(),
                "--items".to_string(),
                anchors.items.join(","),
            ];
            push_opt(&mut v, "--indexing-budget", anchors.indexing_budget);
            v
        }
        RestructureCommand::Verify(verify) => vec![
            "verify".to_string(),
            "--against".to_string(),
            verify.against,
        ],
    }
}

fn push_flag(args: &mut Vec<String>, name: &str, on: bool) {
    if on {
        args.push(name.to_string());
    }
}

fn push_opt<T: std::fmt::Display>(args: &mut Vec<String>, name: &str, value: Option<T>) {
    if let Some(v) = value {
        args.push(name.to_string());
        args.push(v.to_string());
    }
}
