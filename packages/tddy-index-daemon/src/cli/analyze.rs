//! What `analyze` may say, as the four analysis requests.
//!
//! Its own module rather than more of [`super`] for one reason: the restructure half is already
//! five subcommands, and a single file holding both halves' argument structs would be past this
//! repo's length budget while saying nothing that needs saying together. The shape is identical —
//! a subcommand becomes the proto request struct its RPC carries, and nothing here runs anything.
//!
//! The flag names are `tddy-tools analyze`'s (`--path`, `--coverage-dir`, `--out`,
//! `--min-signature`, `--subset-ratio`, `--include-test-sources`), deliberately: the same
//! operation asked for through two front ends should be asked for in the same words, and an
//! operator moving from the one-shot tool to the daemon should not have to relearn them.

use std::path::PathBuf;

use clap::{Args, Subcommand};
use tddy_index_daemon::proto::code_index::{
    ComplexityRequest, CoverageRequest, DuplicateTestsRequest, ReportRequest,
};

use super::{named, Requested, WorkspaceRoot};

/// Where duplicate-tests pages go when the operator names no directory.
///
/// Resolved here rather than by the service, which takes `out_dir` as given: a server that
/// substituted a default would write somewhere its caller did not ask for. This is the same
/// default `tddy-tools analyze duplicate-tests` applies, for the same reason — the pages belong
/// beside the capture they describe.
const PAGES_BESIDE_THE_CAPTURE: &str = "duplicate-tests";

#[derive(Subcommand)]
pub(crate) enum AnalyzeCommand {
    /// Build the instrumented tests, run each one and write its coverage profile.
    Coverage(CoverageArgs),
    /// Join a capture against the complexity of the tree it measured, and write the leaderboard.
    Report(ReportArgs),
    /// Report tests whose coverage signatures are identical or contained in another's.
    DuplicateTests(DuplicateTestsArgs),
    /// Score every function in one source file for cyclomatic complexity.
    Complexity(ComplexityArgs),
}

#[derive(Args)]
pub(crate) struct CoverageArgs {
    #[command(flatten)]
    root: WorkspaceRoot,

    /// The crate to instrument: a directory holding a Cargo.toml, or the manifest itself.
    /// Absolute, or relative to the workspace root.
    #[arg(long, value_name = "DIR")]
    path: PathBuf,

    /// Where the per-test profiles and the denominator are written, absolute or relative to the
    /// workspace root.
    #[arg(long, value_name = "DIR")]
    coverage_dir: PathBuf,
}

#[derive(Args)]
pub(crate) struct ReportArgs {
    #[command(flatten)]
    root: WorkspaceRoot,

    /// The capture to report on — the directory a `coverage` run wrote.
    #[arg(long, value_name = "DIR")]
    coverage_dir: PathBuf,

    /// The tree whose sources are scored for complexity, which is what turns a coverage percentage
    /// into a CRAP score. Often but not necessarily the crate that was captured.
    #[arg(long, value_name = "DIR")]
    path: PathBuf,
}

#[derive(Args)]
pub(crate) struct DuplicateTestsArgs {
    #[command(flatten)]
    root: WorkspaceRoot,

    /// The capture to read, with its `per-test/` artifacts.
    #[arg(long, value_name = "DIR")]
    coverage_dir: PathBuf,

    /// Where the HTML pages are written (default: `<coverage-dir>/duplicate-tests`).
    #[arg(long, value_name = "DIR")]
    out: Option<PathBuf>,

    /// Smallest signature worth grouping.
    #[arg(long, default_value_t = 5, value_name = "KEYS")]
    min_signature: u32,

    /// How much of a test's signature another must cover for it to count as a superset, 0 to 1.
    #[arg(long, default_value_t = 0.5, value_name = "RATIO")]
    subset_ratio: f64,

    /// Count coverage of the test sources themselves, not only of production sources.
    #[arg(long)]
    include_test_sources: bool,
}

#[derive(Args)]
pub(crate) struct ComplexityArgs {
    #[command(flatten)]
    root: WorkspaceRoot,

    /// The file to score, relative to the workspace root or absolute.
    file: PathBuf,
}

/// The request an `analyze` subcommand carries.
pub(crate) fn requested(command: AnalyzeCommand) -> Result<Requested, String> {
    Ok(match command {
        AnalyzeCommand::Coverage(coverage) => Requested::Coverage(CoverageRequest {
            workspace_root: named(&coverage.root.workspace_root)?,
            crate_path: named(&coverage.path)?,
            coverage_dir: named(&coverage.coverage_dir)?,
        }),
        AnalyzeCommand::Report(report) => Requested::Report(ReportRequest {
            workspace_root: named(&report.root.workspace_root)?,
            coverage_dir: named(&report.coverage_dir)?,
            crate_path: named(&report.path)?,
        }),
        AnalyzeCommand::DuplicateTests(DuplicateTestsArgs {
            root,
            coverage_dir,
            out,
            min_signature,
            subset_ratio,
            include_test_sources,
        }) => {
            let out_dir = out.unwrap_or_else(|| coverage_dir.join(PAGES_BESIDE_THE_CAPTURE));
            Requested::DuplicateTests(DuplicateTestsRequest {
                workspace_root: named(&root.workspace_root)?,
                coverage_dir: named(&coverage_dir)?,
                out_dir: named(&out_dir)?,
                min_signature,
                subset_ratio,
                include_test_sources,
            })
        }
        AnalyzeCommand::Complexity(complexity) => Requested::Complexity(ComplexityRequest {
            workspace_root: named(&complexity.root.workspace_root)?,
            file: named(&complexity.file)?,
        }),
    })
}
