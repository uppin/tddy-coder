//! `analyze` subcommands: coverage capture, CRAP report, duplicate-tests.
//!
//! Moved here from `tddy-tools` by `#unbundle` node 5. The dispatch is a thin shell over
//! [`crate::coverage`] and [`crate::report`], so it belongs with them; `tddy-tools` keeps only
//! the `main.rs` line that routes the subcommand here.

use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::coverage::CaptureProgress;
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "analyze")]
pub struct AnalyzeArgs {
    #[command(subcommand)]
    pub command: AnalyzeCommand,
}

#[derive(Subcommand)]
pub enum AnalyzeCommand {
    /// Build instrumented tests and capture per-test llvm-cov profiles.
    Coverage(AnalyzeCoverageArgs),
    /// Join complexity + coverage and write `report.html`.
    Report(AnalyzeReportArgs),
    /// Detect identical and subset test signatures.
    DuplicateTests(AnalyzeDuplicateTestsArgs),
}

#[derive(Parser)]
pub struct AnalyzeCoverageArgs {
    /// Crate path (directory containing Cargo.toml or the manifest itself).
    #[arg(long)]
    pub path: PathBuf,

    /// Coverage output directory (default: `./coverage`).
    #[arg(long)]
    pub coverage_dir: Option<PathBuf>,
}

#[derive(Parser)]
pub struct AnalyzeReportArgs {
    /// Crate path used to measure cyclomatic complexity.
    #[arg(long)]
    pub path: PathBuf,

    /// Coverage directory containing `rust-coverage-final.json`.
    #[arg(long)]
    pub coverage_dir: PathBuf,
}

#[derive(Parser)]
pub struct AnalyzeDuplicateTestsArgs {
    /// Coverage directory with `per-test/` artifacts.
    #[arg(long)]
    pub coverage_dir: PathBuf,

    /// Output directory for HTML reports (default: `<coverage-dir>/duplicate-tests`).
    #[arg(long)]
    pub out: Option<PathBuf>,

    #[arg(long, default_value_t = 5)]
    pub min_signature: usize,

    #[arg(long, default_value_t = 0.5)]
    pub subset_ratio: f64,

    #[arg(long)]
    pub include_test_sources: bool,
}

pub fn run(args: AnalyzeArgs) -> Result<()> {
    match args.command {
        AnalyzeCommand::Coverage(coverage) => run_coverage(coverage),
        AnalyzeCommand::Report(report) => run_report(report),
        AnalyzeCommand::DuplicateTests(dup) => run_duplicate_tests(dup),
    }
}

fn run_coverage(args: AnalyzeCoverageArgs) -> Result<()> {
    let coverage_dir = args
        .coverage_dir
        .unwrap_or_else(|| PathBuf::from("coverage"));
    let mut renderer = ProgressRenderer::new();
    crate::coverage::capture_coverage(&args.path, &coverage_dir, &mut |event| {
        renderer.render(&event)
    })
    .context("coverage capture failed")?;
    Ok(())
}

/// How often to log a captured test when stderr is not a terminal, so a piped
/// run still shows movement without one line per test.
const PIPED_TEST_LOG_INTERVAL: usize = 25;

/// Renders [`CaptureProgress`] to stderr. Progress belongs to the CLI, not the
/// library, so nothing here runs when `tddy-code-analysis` is used elsewhere.
struct ProgressRenderer {
    started: Instant,
    /// A terminal gets one line rewritten in place; a pipe gets periodic lines.
    interactive: bool,
    /// Width of the last in-place line, so the next one fully overwrites it.
    painted: usize,
}

impl ProgressRenderer {
    fn new() -> Self {
        Self {
            started: Instant::now(),
            interactive: std::io::stderr().is_terminal(),
            painted: 0,
        }
    }

    fn render(&mut self, event: &CaptureProgress<'_>) {
        let elapsed = self.started.elapsed();
        match event {
            CaptureProgress::TestCaptured { index, .. } => {
                let line = progress_line(event, elapsed);
                if self.interactive {
                    self.paint_in_place(&line);
                } else if index % PIPED_TEST_LOG_INTERVAL == 0 {
                    eprintln!("{line}");
                }
            }
            _ => {
                self.clear_in_place();
                eprintln!("{}", progress_line(event, elapsed));
            }
        }
    }

    fn paint_in_place(&mut self, line: &str) {
        eprint!(
            "\r{line}{:width$}",
            "",
            width = self.painted.saturating_sub(line.len())
        );
        let _ = std::io::stderr().flush();
        self.painted = line.len();
    }

    fn clear_in_place(&mut self) {
        if self.interactive && self.painted > 0 {
            eprint!("\r{:width$}\r", "", width = self.painted);
            let _ = std::io::stderr().flush();
            self.painted = 0;
        }
    }
}

/// One line of progress text. Pure, so the wording is testable.
fn progress_line(event: &CaptureProgress<'_>, elapsed: Duration) -> String {
    match event {
        CaptureProgress::BuildStarted => {
            "building instrumented tests (dependencies stay uninstrumented; first run takes a few minutes)".to_string()
        }
        CaptureProgress::BuildFinished { harnesses } => {
            format!("built {harnesses} test {}", plural(*harnesses, "harness", "harnesses"))
        }
        CaptureProgress::HarnessStarted { index, total, spec, tests } => {
            format!("[{index}/{total}] {spec} — {tests} {}", plural(*tests, "test", "tests"))
        }
        CaptureProgress::TestCaptured { index, total, name, status } => {
            let failed = if *status == "passed" { "" } else { " FAILED" };
            format!("    {index}/{total} {name}{failed} [{}]", human(elapsed))
        }
        CaptureProgress::Finished { tests, files } => {
            format!(
                "captured {tests} {} across {files} {} in {}",
                plural(*tests, "test", "tests"),
                plural(*files, "file", "files"),
                human(elapsed)
            )
        }
    }
}

fn plural(count: usize, one: &'static str, many: &'static str) -> &'static str {
    if count == 1 {
        one
    } else {
        many
    }
}

fn human(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs();
    if seconds < 60 {
        format!("{seconds}s")
    } else {
        format!("{}m{:02}s", seconds / 60, seconds % 60)
    }
}

fn run_report(args: AnalyzeReportArgs) -> Result<()> {
    crate::report::generate_report(&args.coverage_dir, &args.path)
        .context("report generation failed")?;
    Ok(())
}

fn run_duplicate_tests(args: AnalyzeDuplicateTestsArgs) -> Result<()> {
    let out = args
        .out
        .unwrap_or_else(|| args.coverage_dir.join("duplicate-tests"));
    crate::report::generate_duplicate_tests_report(
        &args.coverage_dir,
        &out,
        args.min_signature,
        args.subset_ratio,
        args.include_test_sources,
    )
    .context("duplicate-tests analysis failed")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(event: &CaptureProgress<'_>) -> String {
        progress_line(event, Duration::from_secs(75))
    }

    #[test]
    fn the_build_phase_says_why_it_is_slow_rather_than_looking_hung() {
        let text = line(&CaptureProgress::BuildStarted);
        assert!(text.contains("building instrumented tests"), "got: {text}");
        assert!(
            text.contains("minutes"),
            "the silent build is what gets mistaken for a hang: {text}"
        );
    }

    #[test]
    fn a_harness_reports_its_position_and_size() {
        let text = line(&CaptureProgress::HarnessStarted {
            index: 12,
            total: 167,
            spec: "tests/acceptance_daemon.rs",
            tests: 45,
        });
        assert!(text.contains("[12/167]"), "got: {text}");
        assert!(text.contains("tests/acceptance_daemon.rs"), "got: {text}");
        assert!(text.contains("45 tests"), "got: {text}");
    }

    #[test]
    fn a_captured_test_reports_progress_and_elapsed_time() {
        let text = line(&CaptureProgress::TestCaptured {
            index: 3,
            total: 45,
            name: "connects",
            status: "passed",
        });
        assert!(text.contains("3/45"), "got: {text}");
        assert!(text.contains("connects"), "got: {text}");
        assert!(text.contains("1m15s"), "elapsed must be legible: {text}");
    }

    #[test]
    fn a_failing_test_is_called_out_but_does_not_stop_the_capture() {
        let text = line(&CaptureProgress::TestCaptured {
            index: 1,
            total: 2,
            name: "flaky",
            status: "failed",
        });
        assert!(text.contains("FAILED"), "got: {text}");
    }

    #[test]
    fn a_passing_test_is_not_labelled() {
        let text = line(&CaptureProgress::TestCaptured {
            index: 1,
            total: 2,
            name: "ok",
            status: "passed",
        });
        assert!(!text.contains("FAILED"), "got: {text}");
    }

    #[test]
    fn the_summary_counts_tests_and_files() {
        let text = line(&CaptureProgress::Finished {
            tests: 2014,
            files: 109,
        });
        assert!(text.contains("2014 tests"), "got: {text}");
        assert!(text.contains("109 files"), "got: {text}");
    }

    #[test]
    fn counts_of_one_read_as_singular() {
        assert!(line(&CaptureProgress::BuildFinished { harnesses: 1 }).contains("1 test harness"));
        assert!(line(&CaptureProgress::Finished { tests: 1, files: 1 })
            .contains("1 test across 1 file"));
    }

    #[test]
    fn elapsed_under_a_minute_stays_in_seconds() {
        assert_eq!(human(Duration::from_secs(9)), "9s");
        assert_eq!(human(Duration::from_secs(605)), "10m05s");
    }
}
