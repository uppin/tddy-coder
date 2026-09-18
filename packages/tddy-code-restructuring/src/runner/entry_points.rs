//! One entry point per subcommand, the routing table over them, and the registries they resolve
//! through.
//!
//! Each returns its result and reports its running account into the sinks its caller installed;
//! the `runner` module doc states why none of this prints.

use crate::backends::rust::ProgressSink;
use crate::backends::RustBackend;
use crate::crate_move;
use crate::journal::{Journal, OpStatus};
use crate::plan::RefactorKind;
use crate::registry::{BackendRegistry, Workspace};
use crate::{Overlay, Plan, RestructureError, Result};
use std::path::Path;
use std::sync::Arc;
use tddy_lsp::client::LspClient;
use tokio_util::sync::CancellationToken;

use super::budget::{budget_report, files_named_by, measured};
use super::comparison::verify;
use super::options::usage;
use super::rehearsal::{survey_lines, Rehearsal};
use super::{
    commit_operation, open_run, parse_options, refuse_repo_scoped_state, restore_ledger, Command,
    Finding, Options, Outcome, PlanProgress, RunSummary, SnapshotRewrite, StatePaths,
};

/// Dispatch a restructuring subcommand given a raw command line.
///
/// `client` is required for LSP-backed operations (`apply`, `anchors`, `check --deep`), and
/// `cancel` is how those operations learn that whoever asked for them has stopped waiting.
pub fn run(
    root: &Path,
    args: &[String],
    client: Option<Arc<LspClient>>,
    cancel: CancellationToken,
) -> Result<Outcome> {
    let command = args.first().map(String::as_str).unwrap_or_default();

    if !matches!(
        command,
        "apply" | "status" | "check" | "anchors" | "verify" | "snapshot"
    ) {
        return Err(usage(format!("unknown command `{command}`")));
    }

    dispatch(root, parse_options(args)?, client, cancel)
}

/// Dispatch a restructuring subcommand that has already been parsed.
///
/// This is what [`crate::restructure_cli`] calls: clap parsed the command line once, and re-parsing
/// its own output is what `cli_vector` used to do when the CLI lived in another package.
///
/// Returns the run's result rather than printing it, so that the front end decides what a result
/// means and where it goes. A caller serving a protocol on its own stdout — the reason this
/// matters — could not use a dispatch that wrote into that stream.
pub fn dispatch(
    root: &Path,
    options: Options,
    client: Option<Arc<LspClient>>,
    cancel: CancellationToken,
) -> Result<Outcome> {
    match options.command {
        Command::Apply => apply(root, options, client, cancel).map(Outcome::Applied),
        Command::Status => status(root, options).map(Outcome::Status),
        Command::Check => check(root, options, client, cancel).map(Outcome::Checked),
        Command::Anchors => {
            // The file is read off the request before the options move, because an anchor is
            // "this range, in this file" — a range alone is not something a plan can carry.
            let file = options.source()?.to_string_lossy().to_string();
            anchors(root, options, client, cancel).map(|range| Outcome::Anchored { file, range })
        }
        Command::Verify => verify(root, options).map(Outcome::Verified),
        Command::Snapshot => snapshot(root, options).map(Outcome::Snapshotted),
    }
}

/// Build a registry for static checks only (no LSP connection).
fn registry_for_static() -> BackendRegistry {
    let mut registry = BackendRegistry::new();
    registry.register(Box::new(RustBackend::new(
        "/usr/bin/rust-analyzer",
        "/tmp",
        "/tmp",
    )));
    registry
}

/// Build a registry backed by rust-analyzer through the shared LSP client.
///
/// The token is what ends a wait for an index that is still loading: this library states no budget
/// of its own, so the only bound on such a wait is the caller it belongs to.
///
/// Both destinations are the caller's: `progress` takes the server's own indexing lines, and
/// `trace` takes the diagnostic account of a seam — but only when `RESTRUCTURE_TRACE` asks for one,
/// which is read here so that a run gets a trace without every caller having to look.
pub fn registry_for(
    client: Arc<LspClient>,
    cancel: CancellationToken,
    progress: ProgressSink,
    trace: fn(&str),
) -> BackendRegistry {
    let mut registry = BackendRegistry::new();
    let mut rust = RustBackend::from_lsp_client(client, Some(cancel), progress);
    if wants_trace(std::env::var_os(TRACE_VARIABLE).as_deref()) {
        rust = rust.with_trace(trace);
    }
    registry.register(Box::new(rust));
    registry
}

/// Execute a plan against the working tree under `root`.
///
/// Returns what the whole run amounted to; the account of each operation as it lands goes to
/// [`Options::account`] while the run is still going, because that is the only time it is worth
/// anything.
pub fn apply(
    root: &Path,
    options: Options,
    client: Option<Arc<LspClient>>,
    cancel: CancellationToken,
) -> Result<RunSummary> {
    let client = client.ok_or_else(|| {
        RestructureError::MalformedPlan("apply requires a rust-analyzer LSP session".into())
    })?;
    let plan_path = options.plan()?;
    let plan = read_plan(&plan_path)?;
    let paths = StatePaths::for_plan(root, &plan_path)?;

    let mut journal = open_run(&plan, root, &paths, &options)?;
    let mut ledger = restore_ledger(&journal, &paths)?;
    let mut registry = registry_for(client, cancel, Arc::clone(&options.progress), options.trace);
    let start = options.from.unwrap_or_else(|| journal.next_op());
    let total = plan.ops.len();
    (options.progress)(&format!(
        "apply: {total} operation(s){}{}",
        if options.dry_run { ", dry-run" } else { "" },
        if start > 0 {
            format!(", from op {start}")
        } else {
            String::new()
        }
    ));
    let mut overlay = Overlay::new();
    let mut done = 0usize;
    let mut stopped_early = false;

    for (index, op) in plan.ops.iter().enumerate().skip(start) {
        // Honouring `--stop-after` is the run doing what it was told, so it ends the loop rather
        // than raising. Reporting it as a malformed plan — with a usage dump — described a
        // successful partial run as a defective one.
        if options
            .stop_after
            .is_some_and(|limit| index >= start + limit)
        {
            (options.progress)(&format!(
                "stopped after {} operation(s) as requested",
                index - start
            ));
            stopped_early = true;
            break;
        }

        let anchor = ledger.translate_anchor(&op.anchor)?;
        (options.progress)(&format!(
            "op {index} of {total}: resolving {:?} in `{}`",
            op.op,
            anchor.file()
        ));
        let resolved = registry
            .backend_for(Path::new(anchor.file()), op.op)?
            .resolve(
                &op.with_anchor(anchor),
                &Workspace {
                    root,
                    overlay: &overlay,
                },
            )?;

        report_visibility(&options.account, &resolved);

        let files = resolved.edit.changes.len();
        (options.progress)(&format!("op {index} of {total}: resolved {files} file(s)"));
        if options.dry_run {
            (options.account)(&progress_line(
                index,
                done,
                plan.ops.len(),
                op.op,
                files,
                false,
            ));
            ledger.record(&resolved.edit);
            overlay.record(root, &resolved.edit)?;
            done += 1;
            continue;
        }

        (options.progress)(&format!(
            "op {index} of {total}: applying {files} file(s) to disk"
        ));
        commit_operation(index, &resolved, root, &paths, &mut journal, &mut ledger)?;
        // Reported *after* the commit, so a line in the account means the edit is on disk and in
        // the journal. An apply used to report nothing at all — the dry run, where nothing is at
        // stake, was the only mode that spoke.
        (options.account)(&progress_line(
            index,
            done,
            plan.ops.len(),
            op.op,
            files,
            true,
        ));
        done += 1;
    }

    Ok(RunSummary {
        applied: done,
        total: plan.ops.len(),
        stopped_early,
    })
}

/// One line of per-operation progress, in the wording every front end uses for it.
///
/// A thin spelling of [`crate::console::operation`], which is where the line itself lives now:
/// `tddy_tools::index_console` and `tddy_index_daemon::render` render the same line from an event
/// rather than from a [`RefactorKind`], so the published function takes the kind as text and this
/// states it. Two numbers, because they answer different questions and are not interchangeable:
/// `[4/29]` is how far the run has got, and `op 3` is the operation's own index — the one `--from`
/// and `--stop-after` take and the one the journal records.
fn progress_line(
    index: usize,
    done: usize,
    total: usize,
    op: RefactorKind,
    files: usize,
    applied: bool,
) -> String {
    crate::console::operation(index, done, total, &format!("{op:?}"), files, applied)
}

/// Rewrite a plan's snapshot header to the working tree under `root` as it stands.
///
/// Every edit to a snapshotted file invalidates the header, and until this existed recomputing it
/// was a shell pipeline each author had to invent around [`crate::apply::hash_file`] — which is
/// public, and which nothing exposed.
///
/// **Line 1 and nothing else.** The plan is a command log, and a subcommand that rewrote an
/// operation would be editing the author's intent rather than restating what the tree holds. So
/// the operations are carried through as the bytes they arrived as, rather than parsed and
/// re-serialized: a plan is also a file people diff, and a round trip through `serde_json` would
/// renumber its whitespace and reorder its keys for nothing.
///
/// Nothing is written when the header already matches, so a `snapshot` of a current plan leaves
/// its mtime alone.
///
/// Nothing goes to [`Options::progress`] either. That sink carries how far an index has got, and
/// this reads a file and hashes what it names — a run with nothing to wait for has no progress to
/// report, and a line there would be narration about an index that was never consulted.
pub fn snapshot(root: &Path, options: Options) -> Result<SnapshotRewrite> {
    let path = options.plan()?;
    let text = std::fs::read_to_string(&path)?;
    let plan = Plan::parse(&text)?;
    let header = plan.rehashed_header(root)?;

    // The header is the first line that is *not blank*, which is where `Plan::parse` reads it from.
    // Taking "everything before the first newline" instead would rewrite a leading blank line and
    // leave the real header behind to be parsed as an operation.
    let blank: usize = text
        .split_inclusive('\n')
        .take_while(|line| line.trim().is_empty())
        .map(str::len)
        .sum();
    let produced = match text[blank..].split_once('\n') {
        Some((_, operations)) => format!("{}{header}\n{operations}", &text[..blank]),
        None => format!("{}{header}", &text[..blank]),
    };

    let rewritten = produced != text;
    if rewritten {
        std::fs::write(&path, &produced)?;
    }

    Ok(SnapshotRewrite {
        plan: path.to_string_lossy().to_string(),
        paths: plan.snapshot.len(),
        rewritten,
    })
}

/// How far a plan's journal under `root` got.
///
/// `in_flight` discounts the operations that went on to complete — the journal holds a record of
/// each — and `pending` is what the plan still has left.
pub fn status(root: &Path, options: Options) -> Result<PlanProgress> {
    let plan_path = options.plan()?;
    let plan = read_plan(&plan_path)?;
    let paths = StatePaths::for_plan(root, &plan_path)?;
    refuse_repo_scoped_state(root, &paths)?;
    let journal = Journal::load(&paths.journal)?;
    let counted = |wanted: OpStatus| {
        journal
            .records
            .iter()
            .filter(|record| record.status == wanted)
            .count()
    };
    let completed = counted(OpStatus::Completed);

    Ok(PlanProgress {
        completed,
        in_flight: counted(OpStatus::InFlight).saturating_sub(completed),
        pending: plan.ops.len().saturating_sub(completed),
        failed: counted(OpStatus::Failed),
    })
}

/// Everything wrong with a plan, without writing anything.
///
/// A plan with findings is an `Ok` carrying them, not a refusal: a caller that receives them as
/// values decides for itself what they mean — a front end fails the run, a plan author reads the
/// report, and a host serving the check forwards them. Only a plan that could not be *checked* —
/// one that will not parse, or whose snapshot does not match the tree — is an error.
///
/// The two things a check produces that are not findings go to [`Options::account`]: a deep
/// check's blast-radius survey, which is a cost rather than a defect, and the file-budget report,
/// which is a record of where the tree stands.
pub fn check(
    root: &Path,
    options: Options,
    client: Option<Arc<LspClient>>,
    cancel: CancellationToken,
) -> Result<Vec<Finding>> {
    let plan = read_plan(&options.plan()?)?;
    plan.verify_snapshot(root)?;

    let mut registry = if options.deep {
        let client = client.ok_or_else(|| {
            RestructureError::MalformedPlan(
                "deep check requires a rust-analyzer LSP session".into(),
            )
        })?;
        registry_for(client, cancel, Arc::clone(&options.progress), options.trace)
    } else {
        registry_for_static()
    };
    let mut rehearsal = Rehearsal::default();
    let mut findings: Vec<Finding> = Vec::new();
    let total = plan.ops.len();
    (options.progress)(&format!(
        "check: {total} operation(s){}",
        if options.deep { ", deep" } else { "" }
    ));

    let static_workspace = Workspace {
        root,
        overlay: &Overlay::new(),
    };
    for (index, op) in plan.ops.iter().enumerate() {
        if op.op != RefactorKind::MoveModuleToCrate {
            continue;
        }
        if let Err(refusal) = crate_move::move_preconditions(&static_workspace, op) {
            findings.push(Finding {
                operation: index,
                detail: refusal.to_string(),
            });
        }
    }

    // Read across the plan rather than per operation: whether a module's siblings come along is a
    // question about the plan, and an operation that is viable on its own is exactly how a
    // mutually-referencing set gets left half moved.
    for (operation, detail) in crate_move::stranded_siblings(&static_workspace, &plan.ops)? {
        findings.push(Finding { operation, detail });
    }

    for (index, op) in plan.ops.iter().enumerate() {
        (options.progress)(&format!(
            "op {index} of {total}: static check {:?} in `{}`",
            op.op,
            op.anchor.file()
        ));
        let statics = registry
            .backend_for(Path::new(op.anchor.file()), op.op)?
            .check(
                op,
                &Workspace {
                    root,
                    overlay: &Overlay::new(),
                },
            )?;
        let statically_sound = statics.is_empty();
        findings.extend(statics.into_iter().map(|detail| Finding {
            operation: index,
            detail,
        }));

        if !options.deep || !statically_sound {
            continue;
        }

        (options.progress)(&format!(
            "op {index} of {total}: deep resolve {:?} in `{}`",
            op.op,
            op.anchor.file()
        ));
        let rehearsed = rehearsal.rehearse(root, &mut registry, op)?;
        if let Some(survey) = &rehearsed.survey {
            for line in survey_lines(index, survey) {
                (options.account)(&line);
            }
        }
        if let Some(refusal) = rehearsed.refusal {
            findings.push(Finding {
                operation: index,
                detail: refusal,
            });
        }
    }

    if let Some(budget) = options.budget {
        for line in budget_report(&measured(root, &files_named_by(&plan))?, budget) {
            (options.account)(&line);
        }
    }

    Ok(findings)
}

/// The range anchor covering a named run of items, ready for a plan to carry.
pub fn anchors(
    root: &Path,
    options: Options,
    client: Option<Arc<LspClient>>,
    cancel: CancellationToken,
) -> Result<crate::edit::Range> {
    let client = client.ok_or_else(|| {
        RestructureError::MalformedPlan("anchors requires a rust-analyzer LSP session".into())
    })?;
    let source = options.source()?;
    let file = source.to_string_lossy().to_string();

    if options.items.is_empty() {
        return Err(usage("anchors needs --items A,B,C"));
    }

    (options.progress)(&format!(
        "anchors: `{file}` ({} item(s))",
        options.items.len()
    ));
    let overlay = Overlay::new();
    let mut registry = registry_for(client, cancel, Arc::clone(&options.progress), options.trace);
    registry
        .backend_for(&source, crate::plan::RefactorKind::ExtractModule)?
        .anchor_for(
            &file,
            &options.items,
            &Workspace {
                root,
                overlay: &overlay,
            },
        )
}

fn read_plan(path: &Path) -> Result<Plan> {
    Plan::parse(&std::fs::read_to_string(path)?)
}

const TRACE_VARIABLE: &str = "RESTRUCTURE_TRACE";

fn wants_trace(value: Option<&std::ffi::OsStr>) -> bool {
    value.is_some_and(|value| !value.is_empty() && value != "0")
}

/// What an operation had to do beyond the edit itself, as lines in the run's account.
///
/// A widened visibility and a note are consequences a reader has to see while the run is going,
/// and both are already carried back in the [`crate::Resolution`] for a caller that wants them as
/// values — which is what the daemon's own apply loop reads instead of this.
fn report_visibility(account: &ProgressSink, resolved: &crate::Resolution) {
    for change in &resolved.report {
        account(&crate::console::visibility(&crate::console::widening(
            change,
        )));
    }
    for note in &resolved.notes {
        account(&crate::console::note(note));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traces_when_the_variable_is_set_to_anything_meaningful() {
        // Given meaningful trace env values
        // When wants_trace is asked
        // Then tracing is enabled
        assert!(wants_trace(Some(std::ffi::OsStr::new("1"))));
        assert!(wants_trace(Some(std::ffi::OsStr::new("verbose"))));
    }

    #[test]
    fn stays_silent_unless_asked() {
        // Given unset, empty, or zero trace env values
        // When wants_trace is asked
        // Then tracing stays off
        assert!(!wants_trace(None));
        assert!(!wants_trace(Some(std::ffi::OsStr::new(""))));
        assert!(!wants_trace(Some(std::ffi::OsStr::new("0"))));
    }

    /// An apply that rewrites the tree has to say what it did as it does it. The line is printed
    /// after the commit, so its presence means the edit reached disk.
    ///
    /// What is asserted *here* is that the operation's own kind reaches the line — this is the only
    /// front end that has a [`RefactorKind`] rather than the text of one, so it is the only place
    /// the naming can go wrong. The line's shape is pinned beside the line, in
    /// [`crate::console`].
    #[test]
    fn names_the_operations_own_kind_in_the_line_that_reports_it() {
        // Given the fourth operation of a 29-operation plan, whose index is 3
        let line = progress_line(3, 3, 29, RefactorKind::ExtractModuleToFile, 3, true);

        // Then the line carries how far the run has got, the resumable index, and the edit's width
        assert_eq!(
            line,
            "[4/29] op 3: ExtractModuleToFile -> 3 file(s) applied"
        );
    }

    /// The counter is what the run has completed; the index is what `--from` would resume at. A
    /// plan run with `--from 20` has them far apart, and conflating them would print the wrong
    /// number to resume from.
    #[test]
    fn keeps_the_counter_and_the_index_independent() {
        // Given a run resumed at operation 20, on its first operation
        let line = progress_line(20, 0, 29, RefactorKind::ExtractMethod, 1, true);

        // Then the counter restarts while the index stays absolute
        assert!(line.starts_with("[1/29] op 20:"), "{line}");
    }

    /// The survey is engine-informed: it asks `textDocument/references` which callers exist. The
    /// backend a check builds has to offer that seam, or a deep check would rehearse the move
    /// without ever reporting its blast radius.
    #[test]
    fn offers_the_reference_engine_a_cross_crate_move_survey_needs() {
        // Given the registry a check builds
        let mut registry = registry_for_static();

        // When the backend for a Rust module is asked for its reference engine
        let backend = registry
            .backend_for(
                Path::new("packages/tddy-daemon/src/host_registry.rs"),
                RefactorKind::MoveModuleToCrate,
            )
            .unwrap();

        // Then it offers one
        assert!(
            backend.module_references().is_some(),
            "the Rust backend answers references and must offer the seam a survey needs"
        );
    }
}
