//! `analyze` subcommands: coverage capture, CRAP report, duplicate-tests.

use std::path::PathBuf;

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
    tddy_code_analysis::coverage::capture_coverage(&args.path, &coverage_dir)
        .context("coverage capture failed")?;
    Ok(())
}

fn run_report(args: AnalyzeReportArgs) -> Result<()> {
    tddy_code_analysis::report::generate_report(&args.coverage_dir, &args.path)
        .context("report generation failed")?;
    Ok(())
}

fn run_duplicate_tests(args: AnalyzeDuplicateTestsArgs) -> Result<()> {
    let out = args
        .out
        .unwrap_or_else(|| args.coverage_dir.join("duplicate-tests"));
    tddy_code_analysis::report::generate_duplicate_tests_report(
        &args.coverage_dir,
        &out,
        args.min_signature,
        args.subset_ratio,
        args.include_test_sources,
    )
    .context("duplicate-tests analysis failed")?;
    Ok(())
}
