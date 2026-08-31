//! Acceptance tests for `tddy-tools analyze` subcommands.

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
use std::fs;

fn tddy_tools_bin() -> Command {
    let mut cmd = cargo_bin_cmd!("tddy-tools");
    cmd.env_remove("TDDY_SOCKET");
    cmd
}

#[test]
fn analyze_report_fails_without_coverage_artifacts() {
    // Given
    let dir = tempfile::tempdir().expect("tempdir");
    let coverage_dir = dir.path().join("coverage");
    fs::create_dir_all(&coverage_dir).expect("coverage dir");
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("manifest");
    fs::create_dir_all(dir.path().join("src")).expect("src");
    fs::write(
        dir.path().join("src/lib.rs"),
        "pub fn answer() -> i32 { 42 }\n",
    )
    .expect("lib");

    // When
    let mut cmd = tddy_tools_bin();
    cmd.args([
        "analyze",
        "report",
        "--path",
        dir.path().to_str().unwrap(),
        "--coverage-dir",
        coverage_dir.to_str().unwrap(),
    ]);
    let assert = cmd.assert().failure();

    // Then
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(
        stderr.contains("coverage") || stderr.contains("rust-coverage-final"),
        "stderr must mention missing coverage, got: {stderr}"
    );
}

#[test]
fn analyze_duplicate_tests_fails_without_per_test_dir() {
    // Given
    let dir = tempfile::tempdir().expect("tempdir");
    let coverage_dir = dir.path().join("coverage");
    fs::create_dir_all(&coverage_dir).expect("coverage dir");

    // When
    let mut cmd = tddy_tools_bin();
    cmd.args([
        "analyze",
        "duplicate-tests",
        "--coverage-dir",
        coverage_dir.to_str().unwrap(),
    ]);
    let assert = cmd.assert().failure();

    // Then
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(
        stderr.contains("per-test") || stderr.contains("coverage"),
        "stderr must mention missing artifacts, got: {stderr}"
    );
}
