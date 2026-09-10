//! CLI-facing entry points for restructuring subcommands.
//!
//! The plan is a command log and is never rewritten. Each operation's resolved edit is appended to
//! an event journal, and the position ledger is a projection over that journal — which is what
//! makes an interrupted run resumable.

use crate::apply::{apply_workspace_edit, ensure_git_worktree, git_output, hash_touched_files};
use crate::backends::RustBackend;
use crate::crate_move::{self, Survey};
use crate::journal::{Journal, JournalRecord, OpStatus, ResumeDecision};
use crate::plan::RefactorKind;
use crate::registry::{BackendRegistry, Workspace};
use crate::{LedgerCheckpoint, Overlay, Plan, PositionLedger, RestructureError, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tddy_lsp::client::LspClient;

const USAGE: &str = "\
usage:
  restructure apply  <plan.jsonl> [--dry-run] [--resume] [--from N] [--stop-after N]
                                  [--indexing-budget SECONDS]
  restructure status <plan.jsonl>
  restructure check  <plan.jsonl> [--deep] [--budget LINES] [--indexing-budget SECONDS]
  restructure anchors <file.rs> --items A,B,C [--indexing-budget SECONDS]
  restructure verify --against <git-ref>

  --dry-run     resolve every operation and print the edits without writing anything
  --resume      continue a plan whose journal already exists
  --from N      replay the journal, then begin executing at operation N
  --stop-after N  apply only the first N operations and stop
  --deep        also resolve every operation through the language server, reporting the refusals an
                apply would give, and the blast radius of every cross-crate move. Writes nothing
                either way
  --budget LINES
                report every file the plan names that is longer than LINES. A record of where the
                tree stands, not a gate: the check's verdict is what its findings say either way
  --items A,B,C the items an emitted range anchor must cover, in any order
  --against REF the git ref to compare the working tree's statements against
  --indexing-budget SECONDS
                how long the Rust backend may spend loading the crate graph, once per run";

/// Which subcommand a run is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Apply,
    Status,
    Check,
    Anchors,
    Verify,
}

/// Parsed command-line options for a restructuring run.
pub struct Options {
    pub command: Command,
    /// The one positional argument: a plan for `apply`, `status` and `check`, a source file for
    /// `anchors`, and nothing at all for `verify`.
    pub target: Option<PathBuf>,
    pub dry_run: bool,
    pub resume: bool,
    pub from: Option<usize>,
    pub stop_after: Option<usize>,
    /// Seconds to allow the Rust backend for its one-time index, when the default is not enough.
    pub indexing_budget: Option<u64>,
    /// Whether `check` resolves each operation through the language server as well as reading text.
    pub deep: bool,
    /// The line count above which `check` reports a file the plan names.
    pub budget: Option<usize>,
    /// The items `anchors` must cover.
    pub items: Vec<String>,
    /// The git ref `verify` compares against.
    pub against: Option<String>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            command: Command::Apply,
            target: None,
            dry_run: false,
            resume: false,
            from: None,
            stop_after: None,
            indexing_budget: None,
            deep: false,
            budget: None,
            items: Vec::new(),
            against: None,
        }
    }
}

/// Dispatch a restructuring subcommand.
///
/// `client` is required for LSP-backed operations (`apply`, `anchors`, `check --deep`).
pub fn run(args: &[String], client: Option<Arc<LspClient>>) -> Result<()> {
    let command = args.first().map(String::as_str).unwrap_or_default();

    if !matches!(command, "apply" | "status" | "check" | "anchors" | "verify") {
        return Err(usage(format!("unknown command `{command}`")));
    }

    let options = parse_options(args)?;
    match options.command {
        Command::Apply => apply(options, client),
        Command::Status => status(options),
        Command::Check => check(options, client),
        Command::Anchors => anchors(options, client),
        Command::Verify => verify(options),
    }
}

/// Read a command line into options, taking flags and the positional argument in any order.
pub fn parse_options(args: &[String]) -> Result<Options> {
    let mut options = Options {
        command: command_of(args),
        ..Options::default()
    };

    let mut rest = args[1..].iter();
    while let Some(argument) = rest.next() {
        options.absorb(argument, &mut rest)?;
    }
    Ok(options)
}

/// Which subcommand the first argument names.
pub fn command_of(args: &[String]) -> Command {
    match args.first().map(String::as_str) {
        Some("status") => Command::Status,
        Some("check") => Command::Check,
        Some("anchors") => Command::Anchors,
        Some("verify") => Command::Verify,
        _ => Command::Apply,
    }
}

impl Options {
    /// Absorb one argument, taking a value from `rest` for the flags that carry one.
    fn absorb<'a>(
        &mut self,
        argument: &str,
        rest: &mut impl Iterator<Item = &'a String>,
    ) -> Result<()> {
        match argument {
            "--dry-run" => self.dry_run = true,
            "--deep" => self.deep = true,
            "--resume" => self.resume = true,
            "--from" => self.from = Some(numeric_value(rest.next(), "--from")?),
            "--stop-after" => self.stop_after = Some(numeric_value(rest.next(), "--stop-after")?),
            "--indexing-budget" => {
                self.indexing_budget = Some(numeric_value(rest.next(), "--indexing-budget")?)
            }
            "--budget" => self.budget = Some(numeric_value(rest.next(), "--budget")?),
            "--items" => self.items = comma_separated(rest.next())?,
            "--against" => {
                self.against = Some(
                    rest.next()
                        .ok_or_else(|| usage("--against needs a git ref"))?
                        .clone(),
                )
            }
            flag if flag.starts_with("--") => return Err(usage(format!("unknown flag `{flag}`"))),
            path if self.target.is_none() => self.target = Some(PathBuf::from(path)),
            extra => {
                return Err(usage(format!(
                    "`{extra}` is a second positional argument; a run takes one"
                )))
            }
        }
        Ok(())
    }

    /// The plan a command was given, named as such in the failure.
    pub fn plan(&self) -> Result<PathBuf> {
        self.target
            .clone()
            .ok_or_else(|| usage("a plan file is required"))
    }

    /// The source file a command was given.
    pub fn source(&self) -> Result<PathBuf> {
        self.target
            .clone()
            .ok_or_else(|| usage("a source file is required"))
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
pub fn registry_for(
    client: Arc<LspClient>,
    indexing_budget: Option<u64>,
    progress: fn(&str),
) -> BackendRegistry {
    let mut registry = BackendRegistry::new();
    let mut rust = RustBackend::from_lsp_client(client, indexing_budget, progress);
    if wants_trace(std::env::var_os(TRACE_VARIABLE).as_deref()) {
        rust = rust.with_trace(report_trace);
    }
    registry.register(Box::new(rust));
    registry
}

/// Execute a plan against the working tree.
pub fn apply(options: Options, client: Option<Arc<LspClient>>) -> Result<()> {
    let client = client.ok_or_else(|| {
        RestructureError::MalformedPlan("apply requires a rust-analyzer LSP session".into())
    })?;
    let root = std::env::current_dir()?;
    let plan = read_plan(&options.plan()?)?;
    let paths = StatePaths::under(&root);

    let mut journal = open_run(&plan, &root, &paths, &options)?;
    let mut ledger = restore_ledger(&journal, &paths)?;
    let mut registry = registry_for(client, options.indexing_budget, report_progress);
    let start = options.from.unwrap_or_else(|| journal.next_op());
    let mut overlay = Overlay::new();
    let mut done = 0usize;

    for (index, op) in plan.ops.iter().enumerate().skip(start) {
        // Honouring `--stop-after` is the run doing what it was told, so it ends the loop rather
        // than raising. Reporting it as a malformed plan — with a usage dump — described a
        // successful partial run as a defective one.
        if options
            .stop_after
            .is_some_and(|limit| index >= start + limit)
        {
            println!("   stopped after {} operations as requested", index - start);
            break;
        }

        let anchor = ledger.translate_anchor(&op.anchor)?;
        let resolved = registry
            .backend_for(Path::new(anchor.file()), op.op)?
            .resolve(
                &op.with_anchor(anchor),
                &Workspace {
                    root: &root,
                    overlay: &overlay,
                },
            )?;

        report_visibility(&resolved);

        let files = resolved.edit.changes.len();
        if options.dry_run {
            println!(
                "{}",
                progress_line(index, done, plan.ops.len(), op.op, files, false)
            );
            ledger.record(&resolved.edit);
            overlay.record(&root, &resolved.edit)?;
            done += 1;
            continue;
        }

        commit_operation(index, &resolved, &root, &paths, &mut journal, &mut ledger)?;
        // Printed *after* the commit, so a line on stdout means the edit is on disk and in the
        // journal. An apply used to report nothing at all — the dry run, where nothing is at
        // stake, was the only mode that spoke.
        println!(
            "{}",
            progress_line(index, done, plan.ops.len(), op.op, files, true)
        );
        done += 1;
    }

    println!(
        "{} {done} of {} operations",
        if options.dry_run {
            "resolved"
        } else {
            "applied"
        },
        plan.ops.len()
    );
    Ok(())
}

/// One line of per-operation progress.
///
/// Two numbers, because they answer different questions and are not interchangeable: `[4/29]` is
/// how far the run has got, and `op 3` is the operation's own index — the one `--from` and
/// `--stop-after` take and the one the journal records. Printing only a human counter would make
/// the number in the log the wrong number to resume from.
fn progress_line(
    index: usize,
    done: usize,
    total: usize,
    op: RefactorKind,
    files: usize,
    applied: bool,
) -> String {
    format!(
        "[{}/{total}] op {index}: {op:?} -> {files} file(s) {}",
        done + 1,
        if applied { "applied" } else { "resolved" }
    )
}

/// Report journal progress for a plan.
pub fn status(options: Options) -> Result<()> {
    let root = std::env::current_dir()?;
    let plan = read_plan(&options.plan()?)?;
    let journal = Journal::load(&StatePaths::under(&root).journal)?;

    let completed = journal
        .records
        .iter()
        .filter(|r| r.status == OpStatus::Completed)
        .count();
    let in_flight = journal
        .records
        .iter()
        .filter(|r| r.status == OpStatus::InFlight)
        .count();
    let failed = journal
        .records
        .iter()
        .filter(|r| r.status == OpStatus::Failed)
        .count();

    println!("completed {completed}");
    println!("in_flight {}", in_flight.saturating_sub(completed));
    println!("failed {failed}");
    println!("pending {}", plan.ops.len().saturating_sub(completed));
    Ok(())
}

/// Report everything wrong with a plan without writing anything.
pub fn check(options: Options, client: Option<Arc<LspClient>>) -> Result<()> {
    let root = std::env::current_dir()?;
    let plan = read_plan(&options.plan()?)?;
    plan.verify_snapshot(&root)?;

    let mut registry = if options.deep {
        let client = client.ok_or_else(|| {
            RestructureError::MalformedPlan(
                "deep check requires a rust-analyzer LSP session".into(),
            )
        })?;
        registry_for(client, options.indexing_budget, report_progress)
    } else {
        registry_for_static()
    };
    let mut rehearsal = Rehearsal::default();
    let mut findings = 0usize;

    for (index, op) in plan.ops.iter().enumerate() {
        let statics = registry
            .backend_for(Path::new(op.anchor.file()), op.op)?
            .check(
                op,
                &Workspace {
                    root: &root,
                    overlay: &Overlay::new(),
                },
            )?;
        for finding in &statics {
            println!("{index}: {finding}");
        }
        findings += statics.len();

        if !options.deep || !statics.is_empty() {
            continue;
        }

        let rehearsed = rehearsal.rehearse(&root, &mut registry, op)?;
        if let Some(survey) = &rehearsed.survey {
            for line in survey_lines(index, survey) {
                println!("{line}");
            }
        }
        if let Some(refusal) = rehearsed.refusal {
            println!("{index}: {refusal}");
            findings += 1;
        }
    }

    if let Some(budget) = options.budget {
        for line in budget_report(&measured(&root, &files_named_by(&plan))?, budget) {
            println!("{line}");
        }
    }

    if findings > 0 {
        return Err(RestructureError::MalformedPlan(format!(
            "{findings} finding(s) — see above. Nothing was written."
        )));
    }

    println!("no findings");
    Ok(())
}

/// Emit the range anchor covering a named run of items, ready to paste into a plan.
pub fn anchors(options: Options, client: Option<Arc<LspClient>>) -> Result<()> {
    let client = client.ok_or_else(|| {
        RestructureError::MalformedPlan("anchors requires a rust-analyzer LSP session".into())
    })?;
    let root = std::env::current_dir()?;
    let source = options.source()?;
    let file = source.to_string_lossy().to_string();

    if options.items.is_empty() {
        return Err(usage("anchors needs --items A,B,C"));
    }

    let overlay = Overlay::new();
    let mut registry = registry_for(client, options.indexing_budget, report_progress_aside);
    let range = registry
        .backend_for(&source, crate::plan::RefactorKind::ExtractModule)?
        .anchor_for(
            &file,
            &options.items,
            &Workspace {
                root: &root,
                overlay: &overlay,
            },
        )?;

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

/// Hold the working tree's statements against a git ref's, as multisets.
pub fn verify(options: Options) -> Result<()> {
    let root = std::env::current_dir()?;
    let against = options
        .against
        .clone()
        .ok_or_else(|| usage("verify needs --against <git-ref>"))?;
    ensure_git_worktree(&root)?;

    let before = sources_at(&root, &against)?;
    let after = sources_now(&root)?;
    let comparison = crate::verify::compare(&before, &after);

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

    Err(RestructureError::MalformedPlan(format!(
        "{} statement(s) the tree lost and {} it gained — see above",
        comparison.missing.len(),
        comparison.added.len()
    )))
}

fn commit_operation(
    index: usize,
    resolved: &crate::Resolution,
    root: &Path,
    paths: &StatePaths,
    journal: &mut Journal,
    ledger: &mut PositionLedger,
) -> Result<()> {
    let pre = hash_touched_files(root, &resolved.edit)?;
    journal.append(&paths.journal, JournalRecord::in_flight(index, pre.clone()))?;

    apply_workspace_edit(root, &resolved.edit)?;

    let post = hash_touched_files(root, &resolved.edit)?;
    journal.append(
        &paths.journal,
        JournalRecord::completed(
            index,
            resolved.edit.clone(),
            pre,
            post,
            resolved.report.clone(),
            resolved.notes.clone(),
        ),
    )?;

    ledger.record(&resolved.edit);
    LedgerCheckpoint {
        op: index,
        ledger: journal.fold_through(index),
    }
    .write(&paths.ledger)
}

#[derive(Default)]
struct Rehearsal {
    ledger: PositionLedger,
    overlay: Overlay,
}

/// What rehearsing one operation against the language server found.
struct Rehearsed {
    /// The blast radius, for the one operation that has one to report before it is performed.
    survey: Option<Survey>,
    /// The refusal an apply would give, where it would give one.
    refusal: Option<String>,
}

impl Rehearsal {
    fn rehearse(
        &mut self,
        root: &Path,
        registry: &mut BackendRegistry,
        op: &crate::plan::RefactorOp,
    ) -> Result<Rehearsed> {
        let anchor = self.ledger.translate_anchor(&op.anchor)?;
        let at = op.with_anchor(anchor);

        // Surveyed before it is resolved, because the two answer different questions: a refusal says
        // the move cannot happen, and the survey says what it would cost if it can. A plan author
        // who gets only the first has to run an apply to learn the second.
        let survey = match self.survey(root, registry, &at) {
            Ok(survey) => survey,
            Err(refusal) => {
                return Ok(Rehearsed {
                    survey: None,
                    refusal: Some(refusal.to_string()),
                })
            }
        };

        let resolved = registry
            .backend_for(Path::new(at.anchor.file()), at.op)?
            .resolve(
                &at,
                &Workspace {
                    root,
                    overlay: &self.overlay,
                },
            );

        match resolved {
            Ok(resolved) => {
                self.ledger.record(&resolved.edit);
                self.overlay.record(root, &resolved.edit)?;
                Ok(Rehearsed {
                    survey,
                    refusal: None,
                })
            }
            Err(refusal) => Ok(Rehearsed {
                survey,
                refusal: Some(refusal.to_string()),
            }),
        }
    }

    /// The blast radius of a cross-crate move, asked of the backend's own reference engine.
    ///
    /// Only `move_module_to_crate` has one worth reporting separately: every other operation's edits
    /// are whatever its assist returns, so a survey of one would be a second name for the resolution
    /// the rehearsal is about to take anyway. This costs the move a second `textDocument/references`
    /// pass on top of the one its resolution makes — which is the price of reporting the radius and
    /// the refusals in a run that writes nothing either way.
    fn survey(
        &self,
        root: &Path,
        registry: &mut BackendRegistry,
        op: &crate::plan::RefactorOp,
    ) -> Result<Option<Survey>> {
        if op.op != RefactorKind::MoveModuleToCrate {
            return Ok(None);
        }

        let workspace = Workspace {
            root,
            overlay: &self.overlay,
        };
        let backend = registry.backend_for(Path::new(op.anchor.file()), op.op)?;
        let Some(engine) = backend.module_references() else {
            return Ok(None);
        };

        crate_move::survey(engine, &workspace, op).map(Some)
    }
}

/// A surveyed cross-crate move, as `check --deep` reports it: where the module is going, what
/// reaches it, and the path every caller would need.
///
/// Indented and prefixed rather than numbered like a finding, because a survey is not one — a move
/// with ninety callers is expensive, not defective, and a check that returned non-zero for it would
/// make the report unusable for deciding whether to write the plan that way.
fn survey_lines(index: usize, survey: &Survey) -> Vec<String> {
    let mut lines = vec![format!(
        "   op {index} survey: {} -> {} ({}), {} item(s) reached from outside, {} caller(s)",
        survey.source,
        survey.destination.package,
        survey.destination.extern_name,
        survey.reached_from_outside.len(),
        survey.callers.len()
    )];

    if !survey.reached_from_outside.is_empty() {
        lines.push(format!(
            "      reached from outside: {}",
            survey.reached_from_outside.join(", ")
        ));
    }

    lines.extend(
        survey
            .callers
            .iter()
            .map(|caller| format!("      {}: {} -> {}", caller.path, caller.from, caller.to)),
    );
    lines
}

/// One file a plan names, and how long it is in the tree as it stands.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FileSize {
    path: String,
    lines: usize,
}

/// Every file a plan operates on, once each, in the order it first names one.
///
/// The anchors are the plan's own statement of what it touches, so this is the set the budget is
/// reported over — not the whole tree, which would bury this plan's outcome in the repository's.
fn files_named_by(plan: &Plan) -> Vec<String> {
    let mut named: Vec<String> = Vec::new();
    for op in &plan.ops {
        let file = op.anchor.file();
        if !named.iter().any(|seen| seen == file) {
            named.push(file.to_string());
        }
    }
    named
}

/// How long each named file is, read from the tree the check is running against.
fn measured(root: &Path, paths: &[String]) -> Result<Vec<FileSize>> {
    paths
        .iter()
        .map(|path| {
            let text = std::fs::read_to_string(root.join(path)).map_err(|error| {
                RestructureError::MalformedPlan(format!(
                    "`{path}` cannot be measured against the budget: {error}"
                ))
            })?;
            Ok(FileSize {
                path: path.clone(),
                lines: text.lines().count(),
            })
        })
        .collect()
}

/// The files longer than `budget` lines, longest first — the order a plan author splits them in.
///
/// A file *at* the budget is within it: the budget is a length a file may reach, and the agreed
/// policy is that seams are cut where they are cohesive rather than to hit a number.
fn over_budget(sizes: &[FileSize], budget: usize) -> Vec<FileSize> {
    let mut over: Vec<FileSize> = sizes
        .iter()
        .filter(|file| file.lines > budget)
        .cloned()
        .collect();
    over.sort_by(|left, right| {
        right
            .lines
            .cmp(&left.lines)
            .then_with(|| left.path.cmp(&right.path))
    });
    over
}

/// The file-budget report, as `check --budget LINES` prints it.
fn budget_report(sizes: &[FileSize], budget: usize) -> Vec<String> {
    let over = over_budget(sizes, budget);
    if over.is_empty() {
        return vec![format!(
            "budget: every file the plan names is within {budget} lines"
        )];
    }

    let mut lines = vec![format!(
        "budget: {} of {} file(s) over {budget} lines",
        over.len(),
        sizes.len()
    )];
    lines.extend(over.iter().map(|file| {
        format!(
            "budget: {} is {} lines, {} over",
            file.path,
            file.lines,
            file.lines - budget
        )
    }));
    lines
}

fn sources_at(root: &Path, git_ref: &str) -> Result<BTreeMap<String, String>> {
    let listing = git_output(root, &["ls-tree", "-r", "--name-only", git_ref])?;
    let mut sources = BTreeMap::new();

    for path in listing.lines().filter(|path| is_comparable(path)) {
        let blob = git_output(root, &["show", &format!("{git_ref}:{path}")])?;
        sources.insert(path.to_string(), blob);
    }
    Ok(sources)
}

fn sources_now(root: &Path) -> Result<BTreeMap<String, String>> {
    let listing = git_output(
        root,
        &["ls-files", "--cached", "--others", "--exclude-standard"],
    )?;
    let confined = root.canonicalize()?;
    let mut sources = BTreeMap::new();

    for path in listing.lines().filter(|path| is_comparable(path)) {
        if let Some(absolute) = confined_regular_file(&confined, path) {
            sources.insert(path.to_string(), std::fs::read_to_string(absolute)?);
        }
    }
    Ok(sources)
}

fn confined_regular_file(root: &Path, relative: &str) -> Option<PathBuf> {
    let candidate = root.join(relative);
    if !candidate.symlink_metadata().ok()?.is_file() {
        return None;
    }
    let real = candidate.canonicalize().ok()?;
    real.starts_with(root).then_some(real)
}

fn is_comparable(path: &str) -> bool {
    path.ends_with(".rs") && !path.starts_with("target/") && !path.contains("/target/")
}

fn open_run(plan: &Plan, root: &Path, paths: &StatePaths, options: &Options) -> Result<Journal> {
    ensure_git_worktree(root)?;
    paths.ensure_self_ignoring()?;

    let journal = Journal::load(&paths.journal)?;
    let continuing = options.resume || options.from.is_some();

    if !journal.records.is_empty() && !continuing {
        return Err(RestructureError::JournalExists);
    }
    if !continuing {
        plan.verify_snapshot(root)?;
    }
    if let Some(ResumeDecision::Abort(op)) = journal.resume_decision(root)? {
        return Err(RestructureError::IndeterminateJournal { op });
    }
    Ok(journal)
}

fn restore_ledger(journal: &Journal, paths: &StatePaths) -> Result<PositionLedger> {
    if let Some(checkpoint) = LedgerCheckpoint::load(&paths.ledger)? {
        journal.verify_checkpoint(&checkpoint)?;
    }
    Ok(journal.fold())
}

struct StatePaths {
    journal: PathBuf,
    ledger: PathBuf,
}

impl StatePaths {
    fn under(root: &Path) -> Self {
        let dir = root.join(".restructure");
        Self {
            journal: dir.join("journal.jsonl"),
            ledger: dir.join("ledger.json"),
        }
    }

    fn ensure_self_ignoring(&self) -> Result<()> {
        let dir = self
            .journal
            .parent()
            .expect("state paths live in a directory");
        std::fs::create_dir_all(dir)?;
        std::fs::write(dir.join(".gitignore"), "*\n")?;
        Ok(())
    }
}

fn read_plan(path: &Path) -> Result<Plan> {
    Plan::parse(&std::fs::read_to_string(path)?)
}

fn comma_separated(value: Option<&String>) -> Result<Vec<String>> {
    Ok(value
        .ok_or_else(|| usage("--items needs a comma-separated list"))?
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect())
}

fn numeric_value<T: std::str::FromStr>(value: Option<&String>, flag: &str) -> Result<T> {
    value
        .and_then(|raw| raw.parse().ok())
        .ok_or_else(|| usage(format!("{flag} needs a whole number")))
}

fn usage(reason: impl std::fmt::Display) -> RestructureError {
    RestructureError::MalformedPlan(format!("{reason}\n{USAGE}"))
}

fn report_progress(line: &str) {
    println!("   indexing: {line}");
}

fn report_progress_aside(line: &str) {
    eprintln!("   indexing: {line}");
}

const TRACE_VARIABLE: &str = "RESTRUCTURE_TRACE";

fn wants_trace(value: Option<&std::ffi::OsStr>) -> bool {
    value.is_some_and(|value| !value.is_empty() && value != "0")
}

fn report_trace(line: &str) {
    eprintln!("   trace: {line}");
}

fn report_visibility(resolved: &crate::Resolution) {
    for change in &resolved.report {
        println!(
            "   visibility: `{}` {} -> {}",
            change.item, change.from, change.to
        );
    }
    for note in &resolved.notes {
        println!("   note: {note}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crate_move::{CallerRewrite, Destination};
    use crate::plan::{Anchor, RefactorOp};

    fn args(flags: &[&str]) -> Vec<String> {
        let mut all = vec!["apply".to_string(), "plan.jsonl".to_string()];
        all.extend(flags.iter().map(|flag| flag.to_string()));
        all
    }

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

    #[test]
    fn reads_the_indexing_budget_a_run_was_given() {
        // Given an indexing budget flag
        let options = parse_options(&args(&["--indexing-budget", "900"])).unwrap();

        // Then the budget is parsed
        assert_eq!(options.indexing_budget, Some(900));
    }

    /// An apply that rewrites the tree has to say what it did as it does it. The line is printed
    /// after the commit, so its presence means the edit reached disk.
    #[test]
    fn reports_an_applied_operation_with_both_its_counter_and_its_index() {
        // Given the fourth operation of a 29-operation plan, whose index is 3
        let line = progress_line(3, 3, 29, RefactorKind::ExtractModuleToFile, 3, true);

        // Then the line carries how far the run has got, the resumable index, and the edit's width
        assert_eq!(
            line,
            "[4/29] op 3: ExtractModuleToFile -> 3 file(s) applied"
        );
    }

    /// A dry run resolves without writing, and must not claim to have applied anything.
    #[test]
    fn distinguishes_a_resolved_operation_from_an_applied_one() {
        // Given the same operation resolved rather than applied
        let line = progress_line(3, 3, 29, RefactorKind::ExtractModuleToFile, 3, false);

        // Then it says so
        assert!(line.ends_with("resolved"), "{line}");
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

    #[test]
    fn leaves_the_indexing_budget_at_the_default_when_none_was_given() {
        // Given no indexing budget flag
        let options = parse_options(&args(&[])).unwrap();

        // Then the budget stays unset
        assert_eq!(options.indexing_budget, None);
    }

    #[test]
    fn refuses_an_indexing_budget_that_is_not_a_number() {
        // Given a non-numeric budget
        let outcome = parse_options(&args(&["--indexing-budget", "soon"]));

        // Then parsing fails
        assert!(outcome.is_err());
    }

    #[test]
    fn refuses_an_indexing_budget_with_no_value_after_it() {
        // Given a bare budget flag
        let outcome = parse_options(&args(&["--indexing-budget"]));

        // Then parsing fails
        assert!(outcome.is_err());
    }

    #[test]
    fn reads_a_plan_named_after_a_flag() {
        // Given check --deep plan.jsonl
        let options = parse_options(&[
            "check".to_string(),
            "--deep".to_string(),
            "plan.jsonl".to_string(),
        ])
        .unwrap();

        // Then the plan and deep flag are both read
        assert_eq!(options.target, Some(PathBuf::from("plan.jsonl")));
        assert!(options.deep);
        assert_eq!(options.command, Command::Check);
    }

    #[test]
    fn refuses_a_second_plan_file() {
        // Given two positional plan paths
        let outcome = parse_options(&[
            "apply".to_string(),
            "one.jsonl".to_string(),
            "two.jsonl".to_string(),
        ]);

        // Then parsing fails
        assert!(outcome.is_err());
    }

    #[test]
    fn reads_the_flags_that_were_already_there() {
        // Given dry-run, from, and stop-after flags
        let options =
            parse_options(&args(&["--dry-run", "--from", "3", "--stop-after", "2"])).unwrap();

        // Then each flag is parsed
        assert!(options.dry_run);
        assert_eq!(options.from, Some(3));
        assert_eq!(options.stop_after, Some(2));
    }

    // ---- the file-budget report ----

    fn a_file_of(path: &str, lines: usize) -> FileSize {
        FileSize {
            path: path.to_string(),
            lines,
        }
    }

    fn paths_of(sizes: &[FileSize]) -> Vec<&str> {
        sizes.iter().map(|size| size.path.as_str()).collect()
    }

    fn a_plan_operating_on(files: &[&str]) -> Plan {
        Plan {
            version: 1,
            snapshot: BTreeMap::new(),
            ops: files
                .iter()
                .map(|file| RefactorOp {
                    op: RefactorKind::ExtractModuleToFile,
                    anchor: Anchor::Symbol {
                        file: file.to_string(),
                        path: "an_item".to_string(),
                    },
                    name: None,
                    to: None,
                    variant: None,
                    with_private_deps: false,
                    reexport: None,
                    to_file: false,
                })
                .collect(),
        }
    }

    /// The report a plan author reads to decide what still has to be split: the files over the
    /// budget and only those, longest first, because the longest is the one worth splitting next.
    #[test]
    fn lists_exactly_the_files_over_the_budget_longest_first() {
        // Given three files a plan names, two of them longer than 500 lines
        let named = vec![
            a_file_of("packages/tddy-daemon/src/host_registry.rs", 612),
            a_file_of("packages/tddy-daemon/src/config.rs", 85),
            a_file_of("packages/tddy-daemon/src/connection_service.rs", 2416),
        ];

        // When the budget report is taken at 500 lines
        let over = over_budget(&named, 500);

        // Then
        assert_eq!(
            paths_of(&over),
            [
                "packages/tddy-daemon/src/connection_service.rs",
                "packages/tddy-daemon/src/host_registry.rs",
            ]
        );
    }

    /// The budget is a length a file may reach: 500 lines is within a 500-line budget. Reporting it
    /// as over would ask for a split the agreed policy does not.
    #[test]
    fn keeps_a_file_exactly_at_the_budget_within_it() {
        // Given one file exactly at the budget and one a single line over it
        let named = vec![a_file_of("src/at.rs", 500), a_file_of("src/over.rs", 501)];

        // When
        let over = over_budget(&named, 500);

        // Then
        assert_eq!(paths_of(&over), ["src/over.rs"]);
    }

    /// How far over matters as much as being over: it is the difference between a file to watch and
    /// one to split.
    #[test]
    fn reports_how_far_over_the_budget_each_file_is() {
        // Given a plan naming two files, one of them 112 lines over budget
        let named = vec![a_file_of("src/at.rs", 500), a_file_of("src/over.rs", 612)];

        // When
        let report = budget_report(&named, 500);

        // Then
        assert_eq!(
            report,
            [
                "budget: 1 of 2 file(s) over 500 lines",
                "budget: src/over.rs is 612 lines, 112 over",
            ]
        );
    }

    /// A clean budget is a result, not silence — the report is how the outcome gets recorded.
    #[test]
    fn reports_that_every_file_a_plan_names_is_within_the_budget() {
        // Given two files a plan names, both within a 500-line budget
        let named = vec![a_file_of("src/at.rs", 500), a_file_of("src/small.rs", 12)];

        // When
        let report = budget_report(&named, 500);

        // Then
        assert_eq!(
            report,
            ["budget: every file the plan names is within 500 lines"]
        );
    }

    /// A split plan names the file it is carving up once per operation — 29 times for a
    /// 29-operation plan — and measuring it 29 times would report it 29 times.
    #[test]
    fn names_each_file_a_plan_operates_on_once() {
        // Given a plan with two operations on one file and one on another
        let plan = a_plan_operating_on(&["src/big.rs", "src/big.rs", "src/other.rs"]);

        // When
        let named = files_named_by(&plan);

        // Then
        assert_eq!(named, ["src/big.rs", "src/other.rs"]);
    }

    #[test]
    fn reads_the_file_budget_a_run_was_given() {
        // Given a file-budget flag
        let options = parse_options(&args(&["--budget", "500"])).unwrap();

        // Then the budget is parsed
        assert_eq!(options.budget, Some(500));
    }

    #[test]
    fn leaves_the_file_budget_unset_when_none_was_given() {
        // Given no file-budget flag
        let options = parse_options(&args(&[])).unwrap();

        // Then no budget is reported on
        assert_eq!(options.budget, None);
    }

    #[test]
    fn refuses_a_file_budget_that_is_not_a_number() {
        // Given a non-numeric budget
        let outcome = parse_options(&args(&["--budget", "short"]));

        // Then parsing fails
        assert!(outcome.is_err());
    }

    // ---- the cross-crate move's blast radius ----

    fn a_survey_of_the_host_registry(
        reached_from_outside: &[&str],
        callers: Vec<CallerRewrite>,
    ) -> Survey {
        Survey {
            source: "packages/tddy-daemon/src/host_registry.rs".to_string(),
            destination: Destination {
                dir: "packages/tddy-daemon-kernel".to_string(),
                package: "tddy-daemon-kernel".to_string(),
                extern_name: "tddy_daemon_kernel".to_string(),
            },
            reached_from_outside: reached_from_outside.iter().map(|s| s.to_string()).collect(),
            callers,
        }
    }

    /// `check --deep` exists to answer "what would this cost" before an apply pays for it, and for a
    /// cross-crate move the reference set *is* the cost.
    #[test]
    fn reports_the_blast_radius_of_a_cross_crate_move() {
        // Given a surveyed move of one item, reached from one caller
        let survey = a_survey_of_the_host_registry(
            &["HostRegistry"],
            vec![CallerRewrite {
                path: "packages/tddy-daemon/src/runtime.rs".to_string(),
                from: "crate::host_registry::HostRegistry".to_string(),
                to: "tddy_daemon_kernel::host_registry::HostRegistry".to_string(),
            }],
        );

        // When the eighth operation of a plan is surveyed
        let lines = survey_lines(7, &survey);

        // Then the destination, the reached items and each caller's new path are all reported
        assert_eq!(
            lines,
            [
                "   op 7 survey: packages/tddy-daemon/src/host_registry.rs -> tddy-daemon-kernel \
                 (tddy_daemon_kernel), 1 item(s) reached from outside, 1 caller(s)",
                "      reached from outside: HostRegistry",
                "      packages/tddy-daemon/src/runtime.rs: crate::host_registry::HostRegistry -> \
                 tddy_daemon_kernel::host_registry::HostRegistry",
            ]
        );
    }

    /// A module nothing outside reaches is the move a reviewer can wave through, and the report says
    /// so in the same shape rather than by staying quiet.
    #[test]
    fn reports_a_cross_crate_move_no_caller_reaches() {
        // Given a surveyed move nothing outside the module names
        let survey = a_survey_of_the_host_registry(&[], Vec::new());

        // When
        let lines = survey_lines(0, &survey);

        // Then the header stands alone, with nothing claimed about callers
        assert_eq!(
            lines,
            [
                "   op 0 survey: packages/tddy-daemon/src/host_registry.rs -> tddy-daemon-kernel \
              (tddy_daemon_kernel), 0 item(s) reached from outside, 0 caller(s)"
            ]
        );
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
