//! The shape of the `restructure` command line, and the options one run of it asks for.
//!
//! Split from [`crate::restructure_cli`] because the two answer different questions: this module
//! is what a command line may say, and that one is what running it does and what its answer looks
//! like on a console. Nothing here prints — the front end owns the console, and a caller that is
//! not a console reaches [`crate::runner`] directly.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

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
}

#[derive(Parser)]
pub struct RestructureAnchorsArgs {
    pub file: PathBuf,

    #[arg(long, value_delimiter = ',')]
    pub items: Vec<String>,
}

#[derive(Parser)]
pub struct RestructureVerifyArgs {
    #[arg(long)]
    pub against: String,
}

/// The parsed subcommand as the runner's own options.
///
/// One `match` and no strings: this is what replaced `cli_vector`, which turned these same fields
/// back into `--flag value` pairs for the runner to re-parse.
///
/// What comes out is silent: where a run's live account goes is the front end's answer, installed
/// by [`crate::restructure_cli`], because the library says nothing unless a caller asks it to.
pub(crate) fn options_for(args: RestructureArgs) -> Options {
    match args.command {
        RestructureCommand::Apply(plan) => Options {
            command: Command::Apply,
            target: Some(plan.plan),
            dry_run: plan.dry_run,
            resume: plan.resume,
            from: plan.from,
            stop_after: plan.stop_after,
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
            ..Options::default()
        },
        RestructureCommand::Anchors(anchors) => Options {
            command: Command::Anchors,
            target: Some(anchors.file),
            items: normalised_items(anchors.items),
            ..Options::default()
        },
        RestructureCommand::Verify(verify) => Options {
            command: Command::Verify,
            against: Some(verify.against),
            ..Options::default()
        },
    }
}

/// `--items` as the runner has always received it: trimmed, with empty elements dropped.
///
/// clap's `value_delimiter = ','` splits on the comma and stops there, so `--items "One, Two"`
/// would otherwise resolve an item literally named `" Two"` and `--items "A,,B"` would carry an
/// empty one — a wrong answer with no error. `runner::comma_separated` did this normalisation
/// while the dispatch lived in `tddy-tools`; the call site keeps doing it now that the parsed
/// arguments cross no package boundary.
fn normalised_items(items: Vec<String>) -> Vec<String> {
    items
        .iter()
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
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
        ]);

        // Then the runner receives them as the values clap already parsed
        assert_eq!(options.command, Command::Apply);
        assert_eq!(options.target, Some(PathBuf::from("plan.jsonl")));
        assert!(options.dry_run);
        assert!(options.resume);
        assert_eq!(options.from, Some(3));
        assert_eq!(options.stop_after, Some(7));
    }

    #[test]
    fn a_check_carries_its_depth_and_its_file_length_budget() {
        // Given a deep check with a file-length budget
        let options = parse(&["check", "plan.jsonl", "--deep", "--budget", "500"]);

        // Then the runner receives both
        assert_eq!(options.command, Command::Check);
        assert!(options.deep);
        assert_eq!(options.budget, Some(500));
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
    fn anchors_resolves_an_item_written_with_a_space_after_the_comma() {
        // Given an items list spelled the way a human writes one
        let options = parse(&["anchors", "src/lib.rs", "--items", "One, Two ,Three"]);

        // Then the runner receives the item names, not the whitespace around them
        assert_eq!(options.items, vec!["One", "Two", "Three"]);
    }

    #[test]
    fn anchors_drops_an_empty_element_rather_than_looking_for_an_unnamed_item() {
        // Given an items list with a stray comma at both ends and in the middle
        let options = parse(&["anchors", "src/lib.rs", "--items", ",One,,Two, ,"]);

        // Then only the two named items reach the runner
        assert_eq!(options.items, vec!["One", "Two"]);
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

    /// The withdrawal is a breaking change to the command line, so it has to be a refusal rather
    /// than a flag quietly accepted and ignored: a run that still passed it would look bounded
    /// while waiting for exactly as long as its caller does.
    #[test]
    fn refuses_an_apply_that_still_passes_the_withdrawn_indexing_budget() {
        // Given an apply carrying the withdrawn flag
        let parsed = RestructureArgs::try_parse_from([
            "restructure",
            "apply",
            "plan.jsonl",
            "--indexing-budget",
            "900",
        ]);

        // Then clap refuses the run rather than parsing a number nothing honours
        assert!(parsed.is_err(), "the withdrawn flag was still accepted");
    }
}
