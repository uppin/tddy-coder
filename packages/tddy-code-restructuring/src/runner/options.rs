//! What a run was asked for: the flags, the one positional argument, and the usage a refusal
//! prints.
//!
//! Its own module because the command line is a surface of its own — `restructure_cli` gave up its
//! clap half to `restructure_args` for the same reason — and because every entry point takes
//! [`Options`] without caring how it was read.

use crate::backends::rust::{discard, ProgressSink};
use crate::{RestructureError, Result};
use std::path::PathBuf;

const USAGE: &str = "\
usage:
  restructure apply  <plan.jsonl> [--dry-run] [--resume] [--from N] [--stop-after N]
  restructure status <plan.jsonl>
  restructure check  <plan.jsonl> [--deep] [--budget LINES]
  restructure anchors <file.rs> --items A,B,C
  restructure verify --against <git-ref>
  restructure snapshot <plan.jsonl>

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
  --against REF the git ref to compare the working tree's statements against";

/// Which subcommand a run is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Apply,
    Status,
    Check,
    Anchors,
    Verify,
    Snapshot,
}

/// What a restructuring run was asked for, and where its live account goes.
///
/// The three destinations are here rather than in each entry point's parameter list because they
/// belong to the same thing the flags do — what this one run was asked for — and because they are
/// the caller's, not this library's. Every one of them is silent by default: a library that was
/// not asked to report says nothing, and the only module in this crate that prints is its
/// command-line front end.
pub struct Options {
    pub command: Command,
    /// The one positional argument: a plan for `apply`, `status`, `check` and `snapshot`, a
    /// source file for `anchors`, and nothing at all for `verify`.
    pub target: Option<PathBuf>,
    pub dry_run: bool,
    pub resume: bool,
    pub from: Option<usize>,
    pub stop_after: Option<usize>,
    /// Whether `check` resolves each operation through the language server as well as reading text.
    pub deep: bool,
    /// The line count above which `check` reports a file the plan names.
    pub budget: Option<usize>,
    /// The items `anchors` must cover.
    pub items: Vec<String>,
    /// The position `anchors --at` names: the innermost item enclosing it is what is anchored.
    pub at: Option<crate::edit::Range>,
    /// The git ref `verify` compares against.
    pub against: Option<String>,
    /// Where the language server's own indexing lines go while an operation waits for an index.
    ///
    /// Progress happens *while* a call is in flight and has nowhere to wait, so it needs a sink
    /// rather than a return value — the reasoning `backends::rust` gives for its own sink.
    pub progress: ProgressSink,
    /// Where a run's running account goes: one line per operation an apply lands, the blast radius
    /// a deep check surveys, and the file-budget report.
    ///
    /// A second sink rather than lines on the first, because the two are different claims: one is
    /// the server saying how far it has got, and this is the run saying what it has just done. A
    /// front end that merged them could not tell them apart again. Kept live rather than folded
    /// into the returned value because an apply's account is only worth anything as it happens —
    /// a line for an operation means that operation's edit is already on disk and in the journal.
    pub account: ProgressSink,
    /// Where a diagnostic trace goes when `RESTRUCTURE_TRACE` asks for one.
    ///
    /// A bare pointer rather than a [`ProgressSink`] because a trace has one destination per front
    /// end rather than one per caller — the audience is whoever is working out why a seam behaved
    /// as it did, not the person waiting. Silent by default, like the other two.
    pub trace: fn(&str),
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
            deep: false,
            budget: None,
            items: Vec::new(),
            at: None,
            against: None,
            progress: discard(),
            account: discard(),
            trace: untraced,
        }
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
        Some("snapshot") => Command::Snapshot,
        _ => Command::Apply,
    }
}

impl Options {
    /// Whether this run continues a journal an earlier run left (`--resume`, or `--from`), rather
    /// than starting the plan fresh.
    pub fn continues_a_journal(&self) -> bool {
        self.resume || self.from.is_some()
    }

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

pub(super) fn usage(reason: impl std::fmt::Display) -> RestructureError {
    RestructureError::MalformedPlan(format!("{reason}\n{USAGE}"))
}

/// The trace nothing is told, for a caller that installed no destination for one.
fn untraced(_line: &str) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(flags: &[&str]) -> Vec<String> {
        let mut all = vec!["apply".to_string(), "plan.jsonl".to_string()];
        all.extend(flags.iter().map(|flag| flag.to_string()));
        all
    }

    /// The flag was withdrawn with the budgets it configured. Accepting and ignoring it would be
    /// worse than refusing it: a run would look bounded and wait for as long as its caller does.
    #[test]
    fn refuses_the_withdrawn_indexing_budget_flag_rather_than_accepting_a_number_nothing_honours() {
        // Given a run that still passes the withdrawn flag
        let outcome = parse_options(&args(&["--indexing-budget", "900"]));

        // Then it is refused as an unknown flag, naming it. `Options` carries no `Debug`, so the
        // accepted half is discarded before the refusal is read.
        let error = outcome.map(|_| ()).expect_err("a refusal").to_string();
        assert!(error.contains("--indexing-budget"), "{error}");
    }

    /// Withdrawing the flag must not cost the run its remaining flags: `--from` is parsed by the
    /// same loop and takes a value the same way, and a run still has to be resumable.
    #[test]
    fn still_reads_the_numeric_flags_the_run_does_take() {
        // Given a run resumed at an operation, stopping after a count
        let options = parse_options(&args(&["--from", "3", "--stop-after", "7"])).unwrap();

        // Then both numbers reach the run
        assert_eq!(options.from, Some(3));
        assert_eq!(options.stop_after, Some(7));
    }

    /// The usage text is what a refused run prints, so a withdrawn flag must not still be
    /// advertised there — the next reader would pass it and be refused.
    #[test]
    fn stops_advertising_the_indexing_budget_in_its_usage() {
        // Given the usage a refusal prints
        // When it is read
        // Then it names no indexing budget
        assert!(!USAGE.contains("--indexing-budget"), "{USAGE}");
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
}
