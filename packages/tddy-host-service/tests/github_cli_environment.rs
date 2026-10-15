//! The `gh` the daemon asks as an OS user is given none of the GitHub token variables the daemon
//! inherited — `gh` prefers them over its own login, which is the login a Local gh project means.
//!
//! It sets process-wide variables, so it is the only test in this binary.

use tddy_host_service::GithubCli;
use tddy_testing_commons::gh_cli::a_gh_cli;

fn the_current_os_user() -> String {
    std::env::var("USER").expect("USER must be set to run this test")
}

#[test]
fn the_gh_asked_as_a_user_is_given_none_of_the_daemons_github_tokens() {
    // Given a daemon environment carrying every GitHub token variable gh honours
    for variable in [
        "GH_TOKEN",
        "GITHUB_TOKEN",
        "GH_ENTERPRISE_TOKEN",
        "GITHUB_ENTERPRISE_TOKEN",
    ] {
        std::env::set_var(variable, "from_the_daemons_environment");
    }
    let dir = tempfile::tempdir().unwrap();
    let record = dir.path().join("gh.env");
    let gh = a_gh_cli(dir.path())
        .logged_in_as("ada", 101)
        .recording_env_to(&record)
        .build();

    // When gh is asked who it is logged in as
    GithubCli::at(gh)
        .logged_in_user(&the_current_os_user(), "github.com")
        .expect("gh answers");

    // Then it was started with none of them
    let environment = std::fs::read_to_string(&record).expect("gh recorded its environment");
    let inherited: Vec<&str> = environment
        .lines()
        .filter(|line| line.contains("from_the_daemons_environment"))
        .collect();
    assert_eq!(inherited, Vec::<&str>::new());
}
