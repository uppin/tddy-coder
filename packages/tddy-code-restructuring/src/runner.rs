//! CLI-facing entry points for restructuring subcommands.
//!
//! The plan is a command log and is never rewritten. Each operation's resolved edit is appended to
//! an event journal, and the position ledger is a projection over that journal — which is what
//! makes an interrupted run resumable.

use crate::apply::{apply_workspace_edit, ensure_git_worktree, git_output, hash_touched_files};
use crate::backends::RustBackend;
use crate::journal::{Journal, JournalRecord, OpStatus, ResumeDecision};
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
  restructure check  <plan.jsonl> [--deep] [--indexing-budget SECONDS]
  restructure anchors <file.rs> --items A,B,C [--indexing-budget SECONDS]
  restructure verify --against <git-ref>

  --dry-run     resolve every operation and print the edits without writing anything
  --resume      continue a plan whose journal already exists
  --from N      replay the journal, then begin executing at operation N
  --stop-after N  apply only the first N operations and stop
  --deep        also resolve every operation through the language server, reporting the refusals an
                apply would give. Writes nothing either way
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

    for (index, op) in plan.ops.iter().enumerate().skip(start) {
        if options
            .stop_after
            .is_some_and(|limit| index >= start + limit)
        {
            return Err(usage(format!(
                "stopped after {} operations as requested",
                index - start
            )));
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

        if options.dry_run {
            println!(
                "{index}: {:?} -> {} file(s)",
                op.op,
                resolved.edit.changes.len()
            );
            ledger.record(&resolved.edit);
            overlay.record(&root, &resolved.edit)?;
            continue;
        }

        commit_operation(index, &resolved, &root, &paths, &mut journal, &mut ledger)?;
    }

    Ok(())
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

        if let Some(refusal) = rehearsal.rehearse(&root, &mut registry, op)? {
            println!("{index}: {refusal}");
            findings += 1;
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

impl Rehearsal {
    fn rehearse(
        &mut self,
        root: &Path,
        registry: &mut BackendRegistry,
        op: &crate::plan::RefactorOp,
    ) -> Result<Option<String>> {
        let anchor = self.ledger.translate_anchor(&op.anchor)?;
        let resolved = registry
            .backend_for(Path::new(anchor.file()), op.op)?
            .resolve(
                &op.with_anchor(anchor),
                &Workspace {
                    root,
                    overlay: &self.overlay,
                },
            );

        match resolved {
            Ok(resolved) => {
                self.ledger.record(&resolved.edit);
                self.overlay.record(root, &resolved.edit)?;
                Ok(None)
            }
            Err(refusal) => Ok(Some(refusal.to_string())),
        }
    }
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
}
