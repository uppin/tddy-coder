use assert_cmd::cargo::cargo_bin_cmd;
use predicates::str::contains;

#[test]
fn help_lists_options() {
    // Given the tddy-livekit-screen-capture binary

    // When invoked with --help
    let mut cmd = cargo_bin_cmd!("tddy-livekit-screen-capture");

    // Then the help output mentions the expected options
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(contains("--list"))
        .stdout(contains("--config"))
        .stdout(contains("--fps"));
}
