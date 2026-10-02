//! One entry point per subcommand, the routing table over them, and the registries they resolve
//! through.
//!
//! Each returns its result and reports its running account into the sinks its caller installed;
//! the `runner` module doc states why none of this prints.

use crate::backends::rust::ProgressSink;
use crate::backends::RustBackend;
use crate::crate_move;
use crate::item_anchor;
use crate::item_anchor::{has_item_anchors, item_anchor_at, items_anchor};
use crate::journal::{Journal, OpStatus};
use crate::plan::{Anchor, RefactorKind};
use crate::plan_store::{FlushPolicy, PlanKey, PlanStore};
use crate::registry::{BackendRegistry, Workspace};
use crate::{Overlay, Plan, PositionLedger, RestructureError, Result};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tddy_lsp::client::LspClient;
use tokio_util::sync::CancellationToken;

use super::budget::{budget_report, files_named_by, measured};
use super::comparison::verify;
use super::options::usage;
use super::rehearsal::{survey_lines, Rehearsal};
use super::{
    commit_operation, open_run_gated, parse_options, refuse_a_broken_baseline,
    refuse_a_broken_result, refuse_repo_scoped_state, report_drifted_hints, restore_ledger,
    AppliedRun, Command, Finding, Options, Outcome, PlanProgress, RunSummary, SnapshotRewrite,
    StatePaths,
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
        Command::Anchors => item_anchors(root, options, client, cancel).map(Outcome::ItemAnchored),
        Command::Verify => verify(root, options).map(Outcome::Verified),
        Command::Snapshot => snapshot(root, options).map(Outcome::Snapshotted),
        // Held across requests, so only a process that outlives one has anything to load into: a
        // run with no daemon has a store for its own length and nothing to name afterwards.
        Command::Load => Err(RestructureError::NeedsIndexDaemon {
            command: "load".to_string(),
        }),
        Command::Unload => Err(RestructureError::NeedsIndexDaemon {
            command: "unload".to_string(),
        }),
        Command::Plans => Err(RestructureError::NeedsIndexDaemon {
            command: "plans".to_string(),
        }),
    }
}

/// Apply the plan `key` names from `store`, starting at `from` (or where its journal left off),
/// refreshing the plan's pending operations after each one and flushing it at the end.
///
/// What every front end runs: the daemon over its long-lived store, a one-shot `apply` over a store
/// that lives for the run. The plan is the store's copy, not the file: `options.target` names
/// nothing here, and a file edited since it was loaded changes nothing about what runs.
///
/// **The store's anchors are current.** After each committed operation the plan's pending operations
/// are rewritten for the tree it left and the plan is written back, so what the file says is what
/// the tree holds as of the last completed operation. A run therefore translates anchors through the
/// edits *of this run* only — a ledger folded from the journal would carry the edits of earlier runs
/// a second time — and a resume reads anchors that match the tree it resumes on. The write follows
/// the journal's record of the operation, so a crash between the two leaves the file one operation
/// behind the journal.
///
/// A dry run writes nothing: no journal record, no refresh, no flush. Neither does a run that is
/// refused before its first operation: the plan file is as it was.
///
/// # Errors
///
/// [`RestructureError::PlanChangedOnDisk`] when the plan's file changed since it was loaded, at the
/// first write. The operations committed before it stay on disk and in the journal.
pub fn apply_from_store(
    root: &Path,
    store: &mut PlanStore,
    key: &PlanKey,
    options: Options,
    client: Option<Arc<LspClient>>,
    cancel: CancellationToken,
) -> Result<RunSummary> {
    let client = client.ok_or_else(|| {
        RestructureError::MalformedPlan("apply requires a rust-analyzer LSP session".into())
    })?;
    let summary = apply_held_plan(root, store, key, &options, client, &cancel)?;
    // A run that refused or failed writes nothing here: the operations it did commit wrote the plan
    // as they landed ([`record_applied_op`]), and one that never started must leave the plan file
    // as it found it — "nothing was written" is what its refusal says.
    if !options.dry_run {
        store.flush(key)?;
    }
    Ok(summary)
}

/// A run that has been opened: its journal, the plan it executes, and where in it to start.
///
/// What [`open_plan_run`] hands back, so the loop that drives it — the command line's, which
/// reports lines, and the daemon's, which reports events — starts from the same state.
pub struct PlanRun {
    pub journal: Journal,
    /// The plan to execute, every item anchor lowered into the range it names on the tree the run
    /// starts on.
    pub plan: Plan,
    pub paths: StatePaths,
    /// Translates `plan`'s anchors through the edits **this run** commits — and nothing earlier.
    /// See [`apply_from_store`] for why it does not start from the journal.
    pub ledger: PositionLedger,
    /// The index of the first operation to execute.
    pub start: usize,
}

/// Open the run of `plan`, a plan a store holds at `plan_path`: the journal, with its refusals, and
/// item anchors lowered, in the one order every apply loop uses ([`open_run_resolving_anchors`]).
///
/// Takes the plan rather than the store, so a host whose store is shared can copy the plan out and
/// not hold the store through a baseline compile check that takes minutes.
pub fn open_plan_run(
    plan: &Plan,
    plan_path: &Path,
    root: &Path,
    options: &Options,
    registry: &mut BackendRegistry,
    cancel: &CancellationToken,
) -> Result<PlanRun> {
    let paths = StatePaths::for_plan(root, plan_path)?;
    // Item anchors resolve, and then the baseline compile check runs, both before `.restructure/`
    // exists — see `open_run_resolving_anchors` for why in that order.
    let (journal, lowered) =
        open_run_resolving_anchors(plan, root, &paths, options, registry, || {
            refuse_a_broken_baseline(root, plan, options, cancel)
        })?;
    // Verified, then set aside: the checkpoint must still agree with the journal, but the anchors
    // this run reads are current (see `apply_from_store`), so what translates them is this run's
    // edits alone.
    restore_ledger(&journal, &paths)?;
    let start = match (options.from, &options.from_id) {
        (Some(_), Some(_)) => {
            return Err(usage("--from names an index or an id, not both"));
        }
        (Some(index), None) => index,
        (None, Some(id)) => lowered
            .ops
            .iter()
            .position(|op| op.id.as_ref() == Some(id))
            .ok_or_else(|| {
                RestructureError::MalformedPlan(format!(
                    "--from names the operation `{id}`, which this plan does not have"
                ))
            })?,
        (None, None) => journal.next_op(),
    };
    Ok(PlanRun {
        journal,
        plan: lowered,
        paths,
        ledger: PositionLedger::new(),
        start,
    })
}

fn apply_held_plan(
    root: &Path,
    store: &mut PlanStore,
    key: &PlanKey,
    options: &Options,
    client: Arc<LspClient>,
    cancel: &CancellationToken,
) -> Result<RunSummary> {
    let plan = store
        .get(key)
        .ok_or_else(|| {
            RestructureError::MalformedPlan(format!("{key} is not loaded — load it first"))
        })?
        .plan
        .clone();
    let mut registry = registry_for(
        client,
        cancel.clone(),
        Arc::clone(&options.progress),
        options.trace,
    );
    let PlanRun {
        mut journal,
        plan,
        paths,
        mut ledger,
        start,
    } = open_plan_run(
        &plan,
        &store.path_of(key),
        root,
        options,
        &mut registry,
        cancel,
    )?;
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

        let at = ledger.translate_op(op)?;
        (options.progress)(&format!(
            "op {index} of {total}: resolving {:?} in `{}`",
            op.op,
            at.anchor.file()
        ));
        let resolved = registry
            .backend_for(Path::new(at.anchor.file()), op.op)?
            .resolve(
                &at,
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
        commit_operation(
            index,
            op.id.as_ref(),
            &resolved,
            root,
            &paths,
            &mut journal,
            &mut ledger,
        )?;
        record_applied_op(store, key, index, &resolved.edit, &mut registry)?;
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

    let run = AppliedRun {
        journal: &journal,
        paths: &paths,
        applied: done,
        total,
    };
    refuse_a_broken_result(root, options, run, cancel)?;
    Ok(RunSummary {
        applied: done,
        total: plan.ops.len(),
        stopped_early,
    })
}

/// Bring the plan `key` up to the tree after its operation `index` was committed, and write it
/// back: pending anchors rewritten through the edit ([`PlanStore::refresh_after_op`]), then the
/// flush, synchronously — the journal already says the operation landed, and a plan that lagged it
/// would be read by a resume as describing the tree before the operation.
///
/// What every apply loop calls after [`commit_operation`], the command line's and the daemon's.
pub fn record_applied_op(
    store: &mut PlanStore,
    key: &PlanKey,
    index: usize,
    edit: &crate::WorkspaceEdit,
    resolver: &mut dyn crate::item_anchor::ItemResolver,
) -> Result<()> {
    let id = store
        .get(key)
        .and_then(|held| held.plan.ops.get(index))
        .and_then(|op| op.id.clone())
        .ok_or_else(|| {
            RestructureError::MalformedPlan(format!(
                "{key} has no operation {index} to refresh from"
            ))
        })?;
    store.refresh_after_op(key, &id, edit, resolver)?;
    store.flush(key)
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
///
/// The plan is loaded into a store that lives for this call and runs through
/// [`apply_from_store`], so a run with no daemon still gives its operations ids, refreshes their
/// anchors and writes the plan back — the same thing the daemon does over a store it keeps.
pub fn apply(
    root: &Path,
    options: Options,
    client: Option<Arc<LspClient>>,
    cancel: CancellationToken,
) -> Result<RunSummary> {
    if client.is_none() {
        return Err(RestructureError::MalformedPlan(
            "apply requires a rust-analyzer LSP session".into(),
        ));
    }
    let plan_path = options.plan()?;
    // Nothing but this call reads the store, so there is no later moment for a debounce to wait for:
    // the flushes are the ones the run asks for.
    let mut store = PlanStore::new(
        root,
        FlushPolicy {
            debounce: Duration::ZERO,
        },
    );
    store.load(std::slice::from_ref(&plan_path))?;
    let key = store.key_for(&plan_path)?;
    apply_from_store(root, &mut store, &key, options, client, cancel)
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
        paths: plan.snapshot.len() + plan.files.len(),
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
    status_of_plan(root, &plan_path, &plan)
}

/// [`status`] of a plan the caller already holds — the daemon's, out of its store — rather than one
/// read from `plan_path`, which still says where the plan's run state is keyed.
pub fn status_of_plan(root: &Path, plan_path: &Path, plan: &Plan) -> Result<PlanProgress> {
    let paths = StatePaths::for_plan(root, plan_path)?;
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
    check_plan(root, plan, options, client, cancel)
}

/// [`check`] of a plan the caller already holds — the daemon's, out of its store — rather than one
/// read from `options.target`.
pub fn check_plan(
    root: &Path,
    plan: Plan,
    options: Options,
    client: Option<Arc<LspClient>>,
    cancel: CancellationToken,
) -> Result<Vec<Finding>> {
    plan.verify_snapshot(root)?;
    report_drifted_hints(&plan, root, &options.progress);

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
    let mut findings: Vec<Finding> = Vec::new();
    // Resolved before anything reads an anchor, so every check below sees the ranges an apply would
    // act on. A static check has no server to resolve them with, so it cannot vouch for an
    // operation anchored by item: that is a finding for each, not a quiet skip, because a plan of
    // item anchors passing `check` green would be a claim nothing examined.
    let plan = if !has_item_anchors(&plan) {
        plan
    } else if options.deep {
        item_anchor::resolve_item_anchors(&plan, root, &mut registry)?
    } else {
        findings.extend(unresolvable_without_a_server(&plan));
        plan
    };
    let mut rehearsal = Rehearsal::default();
    let total = plan.ops.len();
    (options.progress)(&format!(
        "check: {total} operation(s){}",
        if options.deep { ", deep" } else { "" }
    ));

    let static_workspace = Workspace {
        root,
        overlay: &Overlay::new(),
    };
    // Every member of a cluster operation, not only the module its anchor names — otherwise
    // `check` passes a set one of whose members `apply` then refuses, which is the parity this
    // whole static pass exists to hold.
    for (operation, detail) in crate_move::unrunnable(&static_workspace, &plan.ops)? {
        findings.push(Finding { operation, detail });
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

/// A finding for each operation a static check cannot examine because an anchor of it names items.
fn unresolvable_without_a_server(plan: &Plan) -> impl Iterator<Item = Finding> + '_ {
    plan.ops
        .iter()
        .enumerate()
        .filter(|(_, op)| {
            op.anchors()
                .any(|anchor| matches!(anchor, Anchor::Item { .. } | Anchor::Items { .. }))
        })
        .map(|(index, op)| Finding {
            operation: index,
            detail: format!(
                "{:?} in `{}` anchors by item, which only a deep check can resolve, so this \
                 static check did not examine it — run `check --deep`",
                op.op,
                op.anchor.file()
            ),
        })
}

/// The anchor `restructure anchors` emits: an `items` anchor over `options.items`, or — with
/// `options.at` — the `item` anchor of the innermost item enclosing that position.
pub fn item_anchors(
    root: &Path,
    options: Options,
    client: Option<Arc<LspClient>>,
    cancel: CancellationToken,
) -> Result<Anchor> {
    let client = client.ok_or_else(|| {
        RestructureError::MalformedPlan("anchors requires a rust-analyzer LSP session".into())
    })?;
    let source = options.source()?;
    let file = source.to_string_lossy().to_string();

    let mut registry = registry_for(client, cancel, Arc::clone(&options.progress), options.trace);
    match options.at {
        Some(_) if !options.items.is_empty() => {
            Err(usage("anchors takes --items or --at, not both"))
        }
        Some(at) => {
            (options.progress)(&format!(
                "anchors: the item enclosing {}:{} in `{file}`",
                at.start.line, at.start.col
            ));
            item_anchor_at(root, &file, at, &mut registry)
        }
        None if options.items.is_empty() => {
            Err(usage("anchors needs --items A,B,C or --at LINE:COL"))
        }
        None => {
            (options.progress)(&format!(
                "anchors: `{file}` ({} item(s))",
                options.items.len()
            ));
            items_anchor(root, &file, &options.items, &mut registry)
        }
    }
}

/// Open a run the way both apply loops must: item anchors resolved, then the baseline compile
/// check, then `.restructure/` written — and the plan to run, with every anchor lowered to the
/// snapshot coordinates the ledger translates, handed back beside the journal.
///
/// One function so the CLI's apply and the daemon's cannot diverge on the order. It is this order
/// because resolving is cheap and refuses for the commonest reasons — an item edited since the plan
/// was written, absent, or not in its file — while the baseline gate takes minutes of `cargo check`;
/// and both come before the first write, so their refusals' "nothing was written" stays true and
/// leaves no `.restructure/` behind.
///
/// Item anchors are resolved against the tree the run *starts* on, because that is the tree the
/// ledger's coordinates are in. A run that continues a journal does not have it: the tree already
/// holds the edits of the operations the journal completed, and coordinates read from it would be
/// translated through those edits a second time. Such a run is refused here — once before the gate
/// from the journal as found, and again against the journal the open returns, since opening a
/// continued run may adopt a repository-scoped one the first look could not see.
///
/// TODO(plan-store): keep item anchors current across runs, which is what lets a resumed run
/// resolve them; until then a resumed run of a plan that has item anchors is refused.
pub fn open_run_resolving_anchors(
    plan: &Plan,
    root: &Path,
    paths: &StatePaths,
    options: &Options,
    registry: &mut BackendRegistry,
    baseline_gate: impl FnOnce() -> Result<()>,
) -> Result<(Journal, Plan)> {
    let (journal, resolved) = open_run_gated(plan, root, paths, options, || {
        let found = Journal::load(&paths.journal)?;
        let resolved = resolve_item_anchors(plan, root, &found, registry)?;
        baseline_gate()?;
        Ok(resolved)
    })?;
    refuse_a_continued_item_plan(plan, &journal)?;
    Ok((journal, resolved))
}

/// `plan` with every item anchor resolved against the tree it is about to run on, refusing a run
/// that continues a journal (see [`open_run_resolving_anchors`]).
pub fn resolve_item_anchors(
    plan: &Plan,
    root: &Path,
    journal: &Journal,
    registry: &mut BackendRegistry,
) -> Result<Plan> {
    if !has_item_anchors(plan) {
        return Ok(plan.clone());
    }
    refuse_a_continued_item_plan(plan, journal)?;
    item_anchor::resolve_item_anchors(plan, root, registry)
}

fn refuse_a_continued_item_plan(plan: &Plan, journal: &Journal) -> Result<()> {
    if has_item_anchors(plan) && journal.next_op() > 0 {
        return Err(RestructureError::ItemAnchorsOnContinuedRun {
            applied: journal.next_op(),
        });
    }
    Ok(())
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
