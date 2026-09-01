//! `restructure` subcommands: apply/check/status/anchors/verify JSONL plans.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tddy_lsp::allowlist::{Language, LspAllowList};
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

    #[arg(long)]
    pub deep: bool,

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
            LspAllowList::rust_only(),
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
        Some(Arc::clone(&service.client))
    } else {
        None
    };

    tokio::task::spawn_blocking(move || tddy_code_restructuring::runner::run(&cli_args, client))
        .await
        .context("restructure task join")?
        .map_err(anyhow::Error::msg)
}

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
