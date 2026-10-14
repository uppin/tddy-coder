//! A command the daemon runs as an OS user inherits none of the GitHub token variables the daemon
//! itself was started with.
//!
//! `gh` prefers `GH_TOKEN` / `GITHUB_TOKEN` (and, for an Enterprise host, `GH_ENTERPRISE_TOKEN` /
//! `GITHUB_ENTERPRISE_TOKEN`) over its own login, so one exported where the daemon runs would
//! silently stand in for the login a Local gh project means — and for whoever's `gh` the command
//! reaches next. The environment is never a credential.
//!
//! It sets process-wide variables, so it is the only test in this binary.

use std::path::Path;

use tddy_daemon_kernel::spawn_as_user::run_output_as_user;

#[cfg(unix)]
#[test]
fn a_command_run_as_a_user_inherits_none_of_the_daemons_github_tokens() {
    // Given a daemon environment carrying every GitHub token variable gh honours
    for variable in [
        "GH_TOKEN",
        "GITHUB_TOKEN",
        "GH_ENTERPRISE_TOKEN",
        "GITHUB_ENTERPRISE_TOKEN",
    ] {
        std::env::set_var(variable, "from_the_daemons_environment");
    }
    let os_user = std::env::var("USER").expect("USER must be set to run this test");

    // When a command is run as the daemon's own OS user
    let run = run_output_as_user(&os_user, Path::new("/usr/bin/env"), &[])
        .expect("the command starts");
    let output = run.result.expect("the command runs");

    // Then it was started with none of them
    let environment = String::from_utf8_lossy(&output.stdout);
    let inherited: Vec<&str> = environment
        .lines()
        .filter(|line| line.contains("from_the_daemons_environment"))
        .collect();
    assert_eq!(inherited, Vec::<&str>::new());
}
