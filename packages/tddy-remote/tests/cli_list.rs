//! Acceptance: `tddy-remote` lists configured authorities (PRD Testing Plan: cli_lists_authorities).

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;

/// Fixture authority id that a real implementation must echo when listing from the test config.
const FIXTURE_AUTHORITY_ID: &str = "acceptance-fixture-authority";

fn write_fixture_config(dir: &std::path::Path) -> std::path::PathBuf {
    let p = dir.join("remote.yaml");
    std::fs::write(
        &p,
        format!(
            r#"authorities:
  - id: "{id}"
    label: "acceptance fixture"
    connect_base_url: "http://127.0.0.1:9"
"#,
            id = FIXTURE_AUTHORITY_ID
        ),
    )
    .expect("write fixture remote config");
    p
}

#[test]
fn cli_lists_authorities() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = write_fixture_config(dir.path());

    let mut cmd = cargo_bin_cmd!("tddy-remote");
    cmd.args([
        "--config",
        cfg.to_str().expect("utf8 config path"),
        "authorities",
        "list",
    ]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains(FIXTURE_AUTHORITY_ID));
}
