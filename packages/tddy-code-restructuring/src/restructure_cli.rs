//! `restructure` subcommands: apply/check/status/anchors/verify JSONL plans.
//!
//! **The one module in this crate that prints.** The library returns its results and reports its
//! live account into the sinks this module installs, because a front end that speaks a protocol on
//! stdout — the daemon serving these same operations — would have its frames corrupted by a
//! library writing into that stream. So rendering a result, and deciding that a result means a
//! failed run, both happen here.
//!
//! Moved here from `tddy-tools` by `#unbundle` node 5, and the move is the point: the dispatch used
//! to re-serialize its parsed clap arguments back into a `Vec<String>` (`cli_vector`) so that
//! [`crate::runner::parse_options`] could parse them a second time on the other side of the package
//! boundary. With the dispatch in the crate that owns the runner, the parsed arguments are handed
//! to [`crate::runner::dispatch`] as parsed.
//!
//! The command line's own shape lives in [`crate::restructure_args`], which this module re-exports
//! so that a binary needs one path for both.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use tddy_lsp::allowlist::{Language, LaunchSpec, LspAllowList};
use tddy_lsp::registry::{LspKey, LspRegistry};
use tddy_task::TaskRegistry;
use tokio_util::sync::CancellationToken;

use crate::backends::rust::ProgressSink;
use crate::restructure_args::options_for;
use crate::runner::{Command, Finding, Options, Outcome};
use crate::verify::Comparison;

pub use crate::restructure_args::{
    RestructureAnchorsArgs, RestructureArgs, RestructureCheckArgs, RestructureCommand,
    RestructurePlanArgs, RestructureVerifyArgs,
};

pub async fn run(args: RestructureArgs) -> Result<()> {
    let mut options = options_for(args);
    install_console(&mut options);
    let cancel = cancelled_on_interrupt();
    let rehearsal = options.dry_run;

    // The one place the process directory becomes a workspace root. The library takes the root as a
    // parameter and reads the process directory nowhere, so that one process can serve several
    // trees; a command line is the case where the operator has already chosen a tree by standing in
    // it, and saying so once here is that choice. Read once so the language server and the run it
    // serves cannot disagree about which tree they are working on.
    let root = std::env::current_dir().context("current_dir")?;

    let client = if needs_lsp_client(&options) {
        let task_registry = TaskRegistry::new();
        let lsp_registry = LspRegistry::new(
            restructure_allow_list(),
            task_registry,
            Duration::from_secs(600),
        );
        let key = LspKey {
            root: root.clone(),
            language: Language::Rust,
        };
        let service = lsp_registry
            .get_or_spawn(key)
            .await
            .context("rust-analyzer LSP")?;
        // The client's own per-request default is sized for interactive queries, and one request
        // against a cold index routinely outlasts it. This is not a budget on the index — nothing
        // bounds that now but the caller — it is how long a single *unanswered* request may hold
        // the blocking thread before the retry loop gets to look at its token again.
        service.client.set_request_timeout(ONE_REQUEST_LIVENESS);
        Some(Arc::clone(&service.client))
    } else {
        None
    };

    let outcome = tokio::task::spawn_blocking(move || {
        crate::runner::dispatch(&root, options, client, cancel)
    })
    .await
    .context("restructure task join")?
    .map_err(anyhow::Error::msg)?;

    report(outcome, rehearsal)
}

/// Write a run's result to the console this front end owns, and say whether the run succeeded.
///
/// This is also where a result becomes an exit status. A check with findings and a comparison that
/// does not hold are both *answered* calls whose answer is a failed run: the library returns them
/// as values because a caller decides what they mean, and what this caller means by them is a
/// non-zero exit — which `main` produces from the error returned here.
fn report(outcome: Outcome, rehearsal: bool) -> Result<()> {
    match outcome {
        Outcome::Applied(summary) => {
            if summary.stopped_early {
                println!(
                    "   stopped after {} operations as requested",
                    summary.applied
                );
            }
            println!(
                "{} {} of {} operations",
                if rehearsal { "resolved" } else { "applied" },
                summary.applied,
                summary.total
            );
            Ok(())
        }
        Outcome::Status(progress) => {
            println!("completed {}", progress.completed);
            println!("in_flight {}", progress.in_flight);
            println!("failed {}", progress.failed);
            println!("pending {}", progress.pending);
            Ok(())
        }
        Outcome::Checked(findings) => report_findings(&findings),
        Outcome::Anchored { file, range } => {
            println!(
                "{}",
                serde_json::json!({
                    "kind": "range",
                    "file": file,
                    "start": { "line": range.start.line, "col": range.start.col },
                    "end": { "line": range.end.line, "col": range.end.col }
                })
            );
            Ok(())
        }
        Outcome::Verified(comparison) => report_comparison(&comparison),
    }
}

/// Every finding a check made, attributed to the operation that caused it.
fn report_findings(findings: &[Finding]) -> Result<()> {
    for finding in findings {
        println!("{}: {}", finding.operation, finding.detail);
    }

    if findings.is_empty() {
        println!("no findings");
        return Ok(());
    }

    Err(anyhow::anyhow!(
        "{} finding(s) — see above. Nothing was written.",
        findings.len()
    ))
}

/// What holding the tree against a git ref found, and whether it held.
fn report_comparison(comparison: &Comparison) -> Result<()> {
    println!(
        "{} statements before, {} after",
        comparison.before, comparison.after
    );
    for statement in &comparison.missing {
        println!("missing: {statement}");
    }
    for statement in &comparison.added {
        println!("added:   {statement}");
    }

    if comparison.holds() {
        println!("every statement accounted for");
        return Ok(());
    }

    Err(anyhow::anyhow!(
        "{} statement(s) the tree lost and {} it gained — see above",
        comparison.missing.len(),
        comparison.added.len()
    ))
}

/// Point a run's live account at the console this front end owns.
///
/// Where it goes depends on the command, not on the process: `anchors` writes a JSON document to
/// stdout for a caller to paste into a plan, and a line of indexing progress landing in the middle
/// of it would make that document unreadable — so its account goes beside the answer instead.
/// Every other command's stdout is prose already, so its account belongs there, where the operator
/// is reading.
fn install_console(options: &mut Options) {
    let (progress, account): (ProgressSink, ProgressSink) = if options.command == Command::Anchors {
        (
            Arc::new(report_indexing_aside),
            Arc::new(report_account_aside),
        )
    } else {
        (Arc::new(report_indexing), Arc::new(report_account))
    };
    options.progress = progress;
    options.account = account;
    options.trace = report_trace;
}

fn report_indexing(line: &str) {
    println!("   indexing: {line}");
}

fn report_indexing_aside(line: &str) {
    eprintln!("   indexing: {line}");
}

fn report_account(line: &str) {
    println!("{line}");
}

fn report_account_aside(line: &str) {
    eprintln!("{line}");
}

/// Where a seam's diagnostic trace goes when `RESTRUCTURE_TRACE` asks for one.
///
/// Stderr for every command, `anchors` included: a trace is for whoever is working out why a seam
/// behaved as it did, and it must not land in the prose — or the JSON — that stdout is carrying.
fn report_trace(line: &str) {
    eprintln!("   trace: {line}");
}

/// A token this run owns, cancelled when the operator interrupts it.
///
/// The library waits for a loading index for as long as its caller is waiting, so for a one-shot
/// command the operator *is* the caller and `^C` is how they say they have stopped. Cancelling
/// rather than dying on the signal is what lets the run unwind through its own refusal — naming
/// where the index got to, with the journal it has written so far left consistent.
///
/// Taking the signal does replace its default disposition for the rest of the process, so a run
/// wedged inside a single request cannot be interrupted again until that request gives up. `SIGTERM`
/// is deliberately left alone, so there is still an immediate way out of that case.
fn cancelled_on_interrupt() -> CancellationToken {
    let cancel = CancellationToken::new();
    let interrupted = cancel.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            interrupted.cancel();
        }
    });
    cancel
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

/// How long one LSP request may go unanswered before the transport gives up on it.
///
/// Not a budget on indexing: the backend's waits span many requests and end on readiness or on
/// cancellation. This bounds a *single* request, which is a liveness question — a server wedged
/// mid-request holds the blocking thread inside the transport, where no cancellation check runs,
/// so the wait has to come back to the retry loop eventually. It is therefore set well above what
/// one request against a cold index takes: a value near the client's interactive default would
/// turn ordinary indexing into a refusal, which is the defect this change exists to remove.
const ONE_REQUEST_LIVENESS: Duration = Duration::from_secs(600);

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
    use clap::Parser;

    fn parse(argv: &[&str]) -> Options {
        let mut all = vec!["restructure"];
        all.extend_from_slice(argv);
        options_for(RestructureArgs::parse_from(all))
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

    /// Kept as an assertion on the number because it is a policy, not an incidental default: a
    /// value anywhere near the client's interactive default (10s) would turn ordinary indexing
    /// into a timeout, which is the defect the budgets were removed to fix.
    #[test]
    fn allows_one_request_minutes_rather_than_the_seconds_an_interactive_query_gets() {
        // Given the per-request wait a run installs on the shared client
        // When it is read
        // Then it is the ten minutes a cold index can take to answer one request
        assert_eq!(ONE_REQUEST_LIVENESS, Duration::from_secs(600));
    }
}
