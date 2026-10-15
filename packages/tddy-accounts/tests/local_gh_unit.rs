//! Which GitHub host a Local gh project asks `gh` about, and the identity of the login it answers.

use tddy_accounts::local_gh::{github_hostname_of, local_gh_git_identity, LocalGhError};
use tddy_accounts::GitIdentity;

#[test]
fn the_github_host_of_an_https_url_is_its_host() {
    // Given an https clone URL
    let git_url = "https://github.com/acme/alpha.git";

    // When its GitHub host is read
    let host = github_hostname_of(git_url);

    // Then it is the URL's host
    assert_eq!(host, Ok("github.com".to_string()));
}

#[test]
fn the_github_host_of_an_scp_style_ssh_url_is_its_host() {
    // Given an scp-style ssh URL on an Enterprise host
    let git_url = "git@github.example.com:acme/alpha.git";

    // When its GitHub host is read
    let host = github_hostname_of(git_url);

    // Then it is the host before the colon
    assert_eq!(host, Ok("github.example.com".to_string()));
}

#[test]
fn the_github_host_of_an_ssh_scheme_url_is_its_host_without_user_or_port() {
    // Given an ssh:// URL naming a user and a port
    let git_url = "ssh://git@github.example.com:2222/acme/alpha.git";

    // When its GitHub host is read
    let host = github_hostname_of(git_url);

    // Then it is the host alone
    assert_eq!(host, Ok("github.example.com".to_string()));
}

#[test]
fn a_git_url_naming_no_host_is_refused_rather_than_assumed_to_be_github_com() {
    // Given a project registered with no git URL
    let git_url = "";

    // When its GitHub host is read
    let host = github_hostname_of(git_url);

    // Then it is refused, naming the URL, and nothing is assumed
    assert_eq!(
        host,
        Err(LocalGhError::UnreadableGitUrl {
            git_url: String::new()
        })
    );
}

#[test]
fn an_unreadable_git_url_is_refused_in_words_that_say_what_to_fix() {
    // Given a refusal for a local path, which names no host
    let refusal = github_hostname_of("/srv/git/alpha.git").expect_err("a path names no host");

    // When it is shown
    let message = refusal.to_string();

    // Then it names the URL and the setting to fix
    assert!(
        message.contains("/srv/git/alpha.git") && message.contains("git URL"),
        "got: {message}"
    );
}

#[test]
fn a_gh_login_commits_under_its_noreply_address() {
    // Given gh logged in as ada, GitHub user 101
    // When her commit identity is derived
    let identity = local_gh_git_identity("ada", 101);

    // Then it is her login and her noreply address, exactly as a vault account's would be
    assert_eq!(
        identity,
        GitIdentity {
            name: "ada".to_string(),
            email: "101+ada@users.noreply.github.com".to_string(),
        }
    );
}

#[test]
fn a_host_that_is_not_a_hostname_is_refused_rather_than_handed_to_gh() {
    // Given URLs whose host position holds something no GitHub host can be
    let git_urls = [
        "ssh://-oProxyCommand=x/acme/alpha.git",
        "https://bad host/acme/alpha.git",
        "-flag:acme/alpha.git",
        "https://git hub.com/acme/alpha.git",
    ];

    for git_url in git_urls {
        // When the GitHub host is read
        let host = github_hostname_of(git_url);

        // Then it is refused, naming the URL
        assert_eq!(
            host,
            Err(LocalGhError::UnreadableGitUrl {
                git_url: git_url.to_string()
            }),
            "{git_url}"
        );
    }
}

#[test]
fn a_bracketed_ipv6_literal_host_is_a_host() {
    // Given an https URL on an IPv6 literal
    let host = github_hostname_of("https://[::1]/acme/alpha.git");

    // Then the literal is kept without its port handling splitting it
    assert_eq!(host, Ok("[::1]".to_string()));
}
