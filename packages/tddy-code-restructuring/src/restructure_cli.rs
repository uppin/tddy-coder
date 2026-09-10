//! `restructure` subcommands: apply/check/status/anchors/verify JSONL plans.
//!
//! Moved here from `tddy-tools` by `#unbundle` node 5, and the move is the point: the dispatch used
//! to re-serialize its parsed clap arguments back into a `Vec<String>` (`cli_vector`) so that
//! [`crate::runner::parse_options`] could parse them a second time on the other side of the package
//! boundary. With the dispatch in the crate that owns the runner, the parsed arguments are handed
//! to [`crate::runner::dispatch`] as parsed.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tddy_lsp::allowlist::{Language, LaunchSpec, LspAllowList};
use tddy_lsp::registry::{LspKey, LspRegistry};
use tddy_task::TaskRegistry;

use crate::runner::{Command, Options};

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
    let options = options_for(args);

    let client = if needs_lsp_client(&options) {
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
            .set_request_timeout(request_timeout(&options));
        Some(Arc::clone(&service.client))
    } else {
        None
    };

    tokio::task::spawn_blocking(move || crate::runner::dispatch(options, client))
        .await
        .context("restructure task join")?
        .map_err(anyhow::Error::msg)
}

/// The parsed subcommand as the runner's own options.
///
/// One `match` and no strings: this is what replaced `cli_vector`, which turned these same fields
/// back into `--flag value` pairs for the runner to re-parse.
fn options_for(args: RestructureArgs) -> Options {
    match args.command {
        RestructureCommand::Apply(plan) => Options {
            command: Command::Apply,
            target: Some(plan.plan),
            dry_run: plan.dry_run,
            resume: plan.resume,
            from: plan.from,
            stop_after: plan.stop_after,
            indexing_budget: plan.indexing_budget,
            ..Options::default()
        },
        RestructureCommand::Status(plan) => Options {
            command: Command::Status,
            target: Some(plan.plan),
            ..Options::default()
        },
        RestructureCommand::Check(check) => Options {
            command: Command::Check,
            target: Some(check.plan),
            deep: check.deep,
            budget: check.budget,
            indexing_budget: check.indexing_budget,
            ..Options::default()
        },
        RestructureCommand::Anchors(anchors) => Options {
            command: Command::Anchors,
            target: Some(anchors.file),
            items: anchors.items,
            indexing_budget: anchors.indexing_budget,
            ..Options::default()
        },
        RestructureCommand::Verify(verify) => Options {
            command: Command::Verify,
            against: Some(verify.against),
            ..Options::default()
        },
    }
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
            .with_capabilities(crate::client_capabilities())
            .with_initialization_options(crate::server_settings()),
    );
    allow
}

/// How long one LSP request may take, taken from `--indexing-budget` where it was given.
///
/// The budget is the caller's statement of how long the whole resolution may take, so no single
/// request inside it should be cut short by a smaller default.
fn request_timeout(options: &Options) -> Duration {
    Duration::from_secs(
        options
            .indexing_budget
            .unwrap_or(DEFAULT_INDEXING_BUDGET_SECONDS),
    )
}

/// The backend's own warm-up budget, used when `--indexing-budget` is absent so that the
/// request wait and the retry loop expire together.
const DEFAULT_INDEXING_BUDGET_SECONDS: u64 = 600;

fn needs_lsp_client(options: &Options) -> bool {
    match options.command {
        Command::Apply | Command::Anchors => true,
        Command::Check => options.deep,
        Command::Status | Command::Verify => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(argv: &[&str]) -> Options {
        let mut all = vec!["restructure"];
        all.extend_from_slice(argv);
        options_for(RestructureArgs::parse_from(all))
    }

    #[test]
    fn an_apply_carries_its_plan_and_flags_through_as_parsed_values() {
        // Given an apply with every flag it takes
        let options = parse(&[
            "apply",
            "plan.jsonl",
            "--dry-run",
            "--resume",
            "--from",
            "3",
            "--stop-after",
            "7",
            "--indexing-budget",
            "900",
        ]);

        // Then the runner receives them as the values clap already parsed
        assert_eq!(options.command, Command::Apply);
        assert_eq!(options.target, Some(PathBuf::from("plan.jsonl")));
        assert!(options.dry_run);
        assert!(options.resume);
        assert_eq!(options.from, Some(3));
        assert_eq!(options.stop_after, Some(7));
        assert_eq!(options.indexing_budget, Some(900));
    }

    #[test]
    fn a_check_carries_its_depth_and_budgets() {
        // Given a deep check with a file-length budget
        let options = parse(&[
            "check",
            "plan.jsonl",
            "--deep",
            "--budget",
            "500",
            "--indexing-budget",
            "1200",
        ]);

        // Then the runner receives all three
        assert_eq!(options.command, Command::Check);
        assert!(options.deep);
        assert_eq!(options.budget, Some(500));
        assert_eq!(options.indexing_budget, Some(1200));
    }

    #[test]
    fn anchors_carries_its_items_as_a_list_rather_than_a_comma_joined_string() {
        // Given an anchors run over three items
        let options = parse(&["anchors", "src/lib.rs", "--items", "One,Two,Three"]);

        // Then the runner receives the list, not a string it has to split again
        assert_eq!(options.command, Command::Anchors);
        assert_eq!(options.target, Some(PathBuf::from("src/lib.rs")));
        assert_eq!(options.items, vec!["One", "Two", "Three"]);
    }

    #[test]
    fn verify_carries_only_the_git_ref_it_compares_against() {
        // Given a verify against a ref
        let options = parse(&["verify", "--against", "HEAD~1"]);

        // Then that ref is what the runner gets, with no plan
        assert_eq!(options.command, Command::Verify);
        assert_eq!(options.against, Some("HEAD~1".to_string()));
        assert_eq!(options.target, None);
    }

    #[test]
    fn the_language_server_is_started_only_for_the_commands_that_resolve_through_it() {
        // Given one run of each shape
        // When each is asked whether it needs rust-analyzer
        // Then apply and anchors do, check only when deep, and status and verify never
        assert!(needs_lsp_client(&parse(&["apply", "plan.jsonl"])));
        assert!(needs_lsp_client(&parse(&[
            "anchors",
            "src/lib.rs",
            "--items",
            "One"
        ])));
        assert!(needs_lsp_client(&parse(&["check", "plan.jsonl", "--deep"])));
        assert!(!needs_lsp_client(&parse(&["check", "plan.jsonl"])));
        assert!(!needs_lsp_client(&parse(&["status", "plan.jsonl"])));
        assert!(!needs_lsp_client(&parse(&["verify", "--against", "HEAD"])));
    }

    #[test]
    fn a_request_may_take_as_long_as_the_indexing_budget_the_run_was_given() {
        // Given a run with an explicit budget, and one without
        let with_budget = parse(&["apply", "plan.jsonl", "--indexing-budget", "900"]);
        let without = parse(&["apply", "plan.jsonl"]);

        // Then a single request may take the whole budget, defaulting to the backend's own
        assert_eq!(request_timeout(&with_budget), Duration::from_secs(900));
        assert_eq!(
            request_timeout(&without),
            Duration::from_secs(DEFAULT_INDEXING_BUDGET_SECONDS)
        );
    }
}
