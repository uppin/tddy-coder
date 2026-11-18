//! A target session's checkout lives under the target user's home and is cut like a local one
//! (`#remote-shell` 10/11).
//!
//! The target is an in-memory remote shell scripted with what the real target would print, and
//! every command the cut runs is recorded, so a test states the exact path and branch that reach
//! the target. No `ssh`, no `git`, no network.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-16-remote-shell-remote-checkout-root.md

use pretty_assertions::assert_eq;
use tddy_git::{
    remote_checkout_root, repository_name_of, setup_remote_worktree, RemoteBranch, RemoteCheckout,
    RemoteCheckoutError, RemoteWorktreeRequest,
};
use tddy_remote_shell::{ExecOutput, InMemoryShell};

/// The project, as its registry names it.
const QAPE_HQ: &str = "git@github.com:wix-private/qape-hq.git";
/// The Coder workspace's user's home: not `dev`'s, and not the daemon's.
const WORKSPACE_HOME: &str = "/home/coder";
const SESSION: &str = "01a1b2c3-remote-dev";

/// A target whose user's home is `home`, answering the home probe as a pty-merging
/// transport delivers it: the shell's start marker and the probe's own sentinels around
/// the home, with the carriage returns a pty adds. The sentinels spell exactly what
/// `remote_checkout_root` prints around `$HOME`.
fn a_target_whose_home_is(home: &'static str) -> InMemoryShell {
    let shell = InMemoryShell::new("coder.remote-dev");
    shell.answers_when(move |request| {
        request.command.contains("$HOME").then(|| {
            ExecOutput::succeeded(
                format!(
                    "tddy-coder-shell: the command is running\r\n\
                     TDDY_REMOTE_HOME_BEGIN:{home}:TDDY_REMOTE_HOME_END\r\n"
                )
                .into_bytes(),
            )
        })
    });
    shell
}

/// That target, answering every git command with success — a clone that exists and fetches.
fn a_workspace_whose_home_is(home: &'static str) -> InMemoryShell {
    let shell = a_target_whose_home_is(home);
    shell.answers_when(|request| {
        request
            .command
            .contains("git ")
            .then(|| ExecOutput::succeeded(Vec::new()))
    });
    shell
}

fn a_new_branch_from(name: &str, base_ref: &str) -> RemoteWorktreeRequest {
    RemoteWorktreeRequest {
        git_url: QAPE_HQ.to_string(),
        session_id: SESSION.to_string(),
        branch: RemoteBranch::NewFrom {
            name: name.to_string(),
            base_ref: base_ref.to_string(),
        },
    }
}

fn the_existing_branch(name: &str) -> RemoteWorktreeRequest {
    RemoteWorktreeRequest {
        git_url: QAPE_HQ.to_string(),
        session_id: SESSION.to_string(),
        branch: RemoteBranch::Existing {
            name: name.to_string(),
        },
    }
}

/// Every command the target ran, as one text, so a test can say what reached it.
fn everything_run_on(shell: &InMemoryShell) -> String {
    shell.commands_run().join("\n")
}

fn the_root() -> String {
    format!("{WORKSPACE_HOME}/.tddy/repos/qape-hq")
}

#[test]
fn the_repository_name_is_the_urls_last_segment_without_git() {
    assert_eq!(
        (
            repository_name_of(QAPE_HQ),
            repository_name_of("https://github.com/wix-private/qape-hq.git"),
            repository_name_of("https://github.com/wix-private/qape-hq"),
        ),
        (
            "qape-hq".to_string(),
            "qape-hq".to_string(),
            "qape-hq".to_string()
        )
    );
}

#[tokio::test]
async fn the_checkout_root_is_under_the_target_users_home() {
    // Given a workspace whose user's home is /home/coder
    let workspace = a_target_whose_home_is(WORKSPACE_HOME);

    // When the root is resolved
    let root = remote_checkout_root(&workspace, QAPE_HQ).await;

    // Then it is that home, tddy's repos directory, the repository — never a path of this host's
    assert_eq!(root, Ok(the_root()));
}

#[tokio::test]
async fn a_target_whose_home_cannot_be_read_is_refused_with_its_words() {
    // Given a target whose shell cannot print its home
    let target = InMemoryShell::new("buildbox");
    target.answers_when(|request| {
        request
            .command
            .contains("$HOME")
            .then(|| ExecOutput::exited(127, b"sh: printf: not found".to_vec()))
    });

    // When
    let root = remote_checkout_root(&target, QAPE_HQ).await;

    // Then the refusal carries what the target said
    assert_eq!(
        root,
        Err(RemoteCheckoutError::HomeUnresolved(
            "sh: printf: not found".to_string()
        ))
    );
}

#[tokio::test]
async fn a_new_branch_is_cut_from_its_base_ref_on_the_target() {
    // Given
    let workspace = a_workspace_whose_home_is(WORKSPACE_HOME);

    // When the start asks for feature/hooks off origin/main
    let checkout = setup_remote_worktree(
        &workspace,
        &a_new_branch_from("feature/hooks", "origin/main"),
    )
    .await;

    // Then the worktree is under the root, on that branch, cut from that base
    let root = the_root();
    assert_eq!(
        checkout,
        Ok(RemoteCheckout {
            root: root.clone(),
            worktree: format!("{root}/.worktrees/{SESSION}"),
            branch: "feature/hooks".to_string(),
        })
    );
    let run = everything_run_on(&workspace);
    assert!(
        run.contains(&format!(
            "worktree add -b 'feature/hooks' '{root}/.worktrees/{SESSION}' 'origin/main'"
        )),
        "the cut names the branch and its base:\n{run}"
    );
}

#[tokio::test]
async fn an_existing_branch_is_checked_out_on_the_target() {
    // Given
    let workspace = a_workspace_whose_home_is(WORKSPACE_HOME);

    // When the start asks for the branch fix/login the clone already has
    let checkout = setup_remote_worktree(&workspace, &the_existing_branch("fix/login")).await;

    // Then that branch is checked out, and no branch is created
    let root = the_root();
    assert_eq!(
        checkout.map(|checkout| checkout.branch),
        Ok("fix/login".to_string())
    );
    let run = everything_run_on(&workspace);
    assert!(
        run.contains(&format!(
            "worktree add '{root}/.worktrees/{SESSION}' 'fix/login'"
        )) && !run.contains(" -b "),
        "the existing branch is checked out as is:\n{run}"
    );
}

#[tokio::test]
async fn a_cut_names_no_tddy_session_branch() {
    // Given
    let workspace = a_workspace_whose_home_is(WORKSPACE_HOME);

    // When
    let _ = setup_remote_worktree(
        &workspace,
        &a_new_branch_from("feature/hooks", "origin/main"),
    )
    .await;

    // Then nothing the target ran names the branch the old cut invented, or the fixture path
    let run = everything_run_on(&workspace);
    assert!(
        !run.contains("tddy-") && !run.contains("/home/dev"),
        "no `tddy-<session>` branch and no fixture path:\n{run}"
    );
}

#[tokio::test]
async fn a_home_answer_without_the_probes_sentinels_is_refused_with_its_words() {
    // Given a target that answers the home probe with words the probe did not frame
    let target = InMemoryShell::new("coder.remote-dev");
    target.answers_when(|request| {
        request
            .command
            .contains("$HOME")
            .then(|| ExecOutput::succeeded(b"some banner the shell printed".to_vec()))
    });

    // When
    let root = remote_checkout_root(&target, QAPE_HQ).await;

    // Then the answer is not taken for a home, and the refusal carries the target's words
    assert_eq!(
        root,
        Err(RemoteCheckoutError::HomeUnresolved(
            "some banner the shell printed".to_string()
        ))
    );
}

#[tokio::test]
async fn a_fetch_that_fails_is_a_fetch_failure_not_an_unreachable_target() {
    // Given a workspace with a clone already there, whose `git fetch` the target refuses
    let workspace = a_target_whose_home_is(WORKSPACE_HOME);
    workspace.answers_when(|request| {
        request
            .command
            .contains("fetch")
            .then(|| ExecOutput::exited(128, b"fatal: unable to connect to github.com".to_vec()))
    });
    workspace.answers_when(|request| {
        // `test -d <root>/.git`: the clone is already there.
        request
            .command
            .contains(".git")
            .then(|| ExecOutput::succeeded(Vec::new()))
    });

    // When
    let checkout = setup_remote_worktree(
        &workspace,
        &a_new_branch_from("feature/hooks", "origin/main"),
    )
    .await;

    // Then the failure names the fetch step and carries the target's words, and no
    // command the cut ran carried the polluted home the pty delivered
    assert_eq!(
        checkout,
        Err(RemoteCheckoutError::Fetch(
            "fatal: unable to connect to github.com".to_string()
        ))
    );
    let shown = RemoteCheckoutError::Fetch("fatal: unable to connect to github.com".to_string())
        .to_string();
    assert!(
        shown.contains("fetching on the target failed") && !shown.contains("unreachable"),
        "the fetch step is named, never unreachable: {shown}"
    );
    let run = everything_run_on(&workspace);
    assert!(
        run.contains(&format!("test -d '{}/.git'", the_root()))
            && !run.contains("tddy-coder-shell")
            && !run.contains('\r'),
        "every later command names the clean home:\n{run}"
    );
}

#[tokio::test]
async fn a_clone_that_fails_reports_the_targets_words() {
    // Given a workspace with no clone yet, whose `git clone` the forge refuses
    let workspace = a_target_whose_home_is(WORKSPACE_HOME);
    workspace.answers_when(|request| {
        request.command.contains("git clone").then(|| {
            ExecOutput::exited(
                128,
                b"fatal: could not read Username for 'https://github.com'".to_vec(),
            )
        })
    });
    workspace.answers_when(|request| {
        // `test -d <root>/.git`: there is no clone yet.
        request
            .command
            .contains(".git")
            .then(|| ExecOutput::exited(1, Vec::new()))
    });

    // When
    let checkout = setup_remote_worktree(
        &workspace,
        &a_new_branch_from("feature/hooks", "origin/main"),
    )
    .await;

    // Then the refusal names the URL and carries git's words from the target
    assert_eq!(
        checkout,
        Err(RemoteCheckoutError::Clone {
            url: QAPE_HQ.to_string(),
            stderr: "fatal: could not read Username for 'https://github.com'".to_string(),
        })
    );
}
