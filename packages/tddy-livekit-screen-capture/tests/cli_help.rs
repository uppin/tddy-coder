use assert_cmd::cargo::cargo_bin_cmd;
use predicates::str::contains;

#[test]
fn help_lists_options() {
    let mut cmd = cargo_bin_cmd!("tddy-livekit-screen-capture");
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(contains("--list"))
        .stdout(contains("--config"))
        .stdout(contains("--fps"));
}
