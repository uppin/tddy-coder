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

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tddy_lsp::allowlist::{Language, LaunchSpec, LspAllowList};
use tddy_lsp::registry::{LspKey, LspRegistry};
use tddy_task::TaskRegistry;
use tokio_util::sync::CancellationToken;

use crate::backends::rust::ProgressSink;
use crate::console;
use crate::restructure_args::options_for;
use crate::runner::{Command, Options, Outcome};

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
        (options.progress)("acquiring shared rust-analyzer client (first run may take minutes)");
        let service = lsp_registry
            .get_or_spawn(key)
            .await
            .context("rust-analyzer LSP")?;
        (options.progress)("rust-analyzer client ready");
        service
            .client
            .set_request_timeout(REQUEST_BOUND_ABOVE_ANY_COLD_INDEX);
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
/// The wording is [`crate::console`]'s, which is what makes the same operation read the same
/// whether it was served cold by this process or warm by the index daemon. What is *this* front
/// end's, and stays here, is where those lines go — stdout, because this is the only module in the
/// crate that owns a console — and what the result means. A check with findings and a comparison
/// that does not hold are both *answered* calls whose answer is a failed run: the library returns
/// them as values because a caller decides what they mean, and what this caller means by them is a
/// non-zero exit, which `main` produces from the error returned here.
fn report(outcome: Outcome, rehearsal: bool) -> Result<()> {
    for line in console::outcome(&outcome, rehearsal) {
        println!("{line}");
    }
    verdict_on(&outcome)
}

/// Whether a result that was *answered* is nonetheless a failed run.
fn verdict_on(outcome: &Outcome) -> Result<()> {
    match outcome {
        Outcome::Checked(findings) if !findings.is_empty() => {
            Err(anyhow::anyhow!(console::findings_refusal(findings.len())))
        }
        Outcome::Verified(comparison) if !comparison.holds() => {
            Err(anyhow::anyhow!(console::comparison_refusal(comparison)))
        }
        _ => Ok(()),
    }
}

/// Point a run's live account at the console this front end owns.
///
/// Two destinations, on a rule #500 got right: the server's narration — how far the index got, which
/// assist is outstanding — is **always** stderr, stamped with the time since the line before it, so
/// stdout carries only the answer. `anchors` writes a JSON document there for a caller to paste into
/// a plan, and an indexing line landing in the middle of it would make that document unreadable;
/// the same holds less dramatically for a check's findings and an apply's summary. `anchors`' own
/// per-operation account therefore also goes aside, because its stdout is the document.
fn install_console(options: &mut Options) {
    let account: ProgressSink = if options.command == Command::Anchors {
        Arc::new(report_account_aside)
    } else {
        Arc::new(report_account)
    };
    // Narration goes to stderr for every command, #500's rule and the better one: stdout then
    // carries only the answer — `anchors`' JSON document, a check's findings, an apply's summary —
    // and stays something a caller can read without filtering.
    let progress = a_stamped_sink("indexing", true);
    options.progress = progress;
    options.account = account;
    options.trace = report_trace;
}

/// A sink that stamps each line with the time since the line before it, and writes it where
/// `aside` says.
///
/// The clock belongs to the sink, not to the process. #500 introduced this stamping over a
/// `static OnceLock<Mutex<Option<Instant>>>`, which is right for a command line — one run, one
/// clock — and wrong for anything serving two callers at once: their deltas would interleave
/// through one shared instant and both accounts would be nonsense. A closure owning its own
/// `Instant` reads identically for the CLI and stays correct when a second caller appears.
fn a_stamped_sink(kind: &'static str, aside: bool) -> ProgressSink {
    let previous: Mutex<Option<Instant>> = Mutex::new(None);
    Arc::new(move |line: &str| {
        let now = Instant::now();
        let stamp = {
            let mut last = previous.lock().expect("the sink's own clock");
            let stamp = step_delta(*last, now);
            *last = Some(now);
            stamp
        };
        let stamped = console::narration(kind, Some(&stamp), line);
        if aside {
            eprintln!("{stamped}");
        } else {
            println!("{stamped}");
        }
    })
}

/// Elapsed time since the previous line, in the units a reader can act on.
///
/// Carried over from #500 unchanged: a phase that took 6 minutes and one that took 60ms want
/// different reactions, and a reader should not have to subtract timestamps to tell them apart.
///
/// **Public because it is the contract between the front ends, not because it is part of a
/// console.** It is pure — instants in, text out, nothing printed — so publishing it leaves this
/// module the only one in the crate that writes to stdout, the invariant
/// `tests/library_returns_its_results.rs` pins. The other front ends are
/// `tddy_tools::index_console`, which narrates the same run served by the index daemon and would
/// otherwise have to restate these units, and `tddy_index_daemon::activity`, which stamps how long
/// a request took in the same vocabulary. A fourth spelling of "+1m30s" is a thing that drifts.
pub fn step_delta(previous: Option<Instant>, now: Instant) -> String {
    let delta = previous
        .map(|earlier| now.duration_since(earlier))
        .unwrap_or(Duration::ZERO);
    let ms = delta.as_millis();
    if ms == 0 {
        "+0ms".to_string()
    } else if ms < 1000 {
        format!("+{ms}ms")
    } else if ms < 60_000 {
        format!("+{:.1}s", delta.as_secs_f64())
    } else {
        format!("+{}m{}s", ms / 60_000, delta.as_secs() % 60)
    }
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
/// Taking the signal does replace its default disposition for the rest of the process, so `^C` no
/// longer kills the run outright — which is why the token has to reach everywhere a run waits,
/// including a request already in flight. It does:
/// [`crate::backends::LspClientBridge`] drives each request with this token and abandons it, at the
/// server as well as here. `SIGTERM` is still deliberately left alone.
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
/// **Not a way out of a wedged request.** That was the old justification, and it no longer holds:
/// [`crate::backends::LspClientBridge`] drives every request with this run's cancellation token,
/// so a `^C` reaches one in flight and the bound is not what an interrupted operator waits on.
///
/// What is left is the reason it must stay *high*. `RustBackend::request_settled` maps an expired
/// bound to "the server is still catching up" and re-issues the request, a bounded number of
/// times — so the bound multiplied by that count is a hard ceiling on how long a cold index may
/// take, whatever the caller is willing to wait. At the client's interactive default (10s) that
/// ceiling is minutes, which is a budget by the back door and the very defect the budgets were
/// removed to fix. Ten minutes puts it clear of any single request against a cold graph, leaving
/// the end of a run where this library wants it: with the caller.
const REQUEST_BOUND_ABOVE_ANY_COLD_INDEX: Duration = Duration::from_secs(600);

fn needs_lsp_client(options: &Options) -> bool {
    match options.command {
        Command::Apply | Command::Anchors => true,
        Command::Check => options.deep,
        Command::Status | Command::Verify => false,
    }
}

#[cfg(test)]
mod tests {
    // Carried over from #500 with the function, which moved here when the library stopped printing.
    // Four cases because each picks a different unit, and a reader acts on the unit: `+250ms` and
    // `+1m30s` want opposite reactions and a single format would bury one of them.
    #[test]
    fn stamps_elapsed_time_since_the_previous_line() {
        let t0 = Instant::now();
        assert_eq!(step_delta(None, t0), "+0ms");
        assert_eq!(
            step_delta(Some(t0), t0 + Duration::from_millis(250)),
            "+250ms"
        );
        assert_eq!(step_delta(Some(t0), t0 + Duration::from_secs(3)), "+3.0s");
        assert_eq!(step_delta(Some(t0), t0 + Duration::from_secs(90)), "+1m30s");
    }

    #[test]
    fn a_stamped_sink_keeps_its_own_clock_rather_than_sharing_one() {
        // Given two sinks, as two concurrent callers would each own
        let first = a_stamped_sink("indexing", true);
        let second = a_stamped_sink("indexing", true);

        // When both are written to, interleaved
        first("one");
        second("two");
        first("three");

        // Then neither panicked on a poisoned shared clock, and each advanced its own. The
        // process-global `static` this replaced would have had both callers' deltas measured
        // against whichever line was written last, by either of them.
        second("four");
    }

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

    /// Kept as an assertion on the number because it is a policy, not an incidental default.
    ///
    /// The bound is no longer how a run gets out of a wedged request — the run's cancellation
    /// token reaches one in flight now — so what is left to justify is that it stays *well clear*
    /// of a cold index. `request_settled` re-issues a request whose bound expired, up to
    /// `CONTENT_MODIFIED_RETRIES` times, so a bound anywhere near the client's interactive default
    /// (10s) puts a hard ceiling of retries × bound on indexing: a budget by the back door, and
    /// the defect removing the budgets was meant to fix.
    #[test]
    fn keeps_the_per_request_bound_far_above_what_a_cold_index_takes_to_answer_one_request() {
        // Given the per-request bound a run installs on the shared client
        // When it is read
        // Then it is the ten minutes a cold index can take to answer one request, not the seconds
        // an interactive query gets
        assert_eq!(REQUEST_BOUND_ABOVE_ANY_COLD_INDEX, Duration::from_secs(600));
    }
}
