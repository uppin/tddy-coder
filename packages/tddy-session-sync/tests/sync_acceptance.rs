//! The sync loop's decisions — AC27-AC32 of `docs/ft/daemon/session-worktree-sync.md`.
//!
//! Every decision the loop makes is taken here without a room and without a daemon: what a record
//! is worth fetching, what a stream of frames reassembles to, what a commit event provokes, and the
//! exact git a first attach and a reconcile run. The last of those is executed against real
//! repositories in temp directories — a command list nothing ever ran is a command list that can be
//! wrong in every way that matters.
//!
//! The one thing not covered here is the fetch itself, which needs a daemon serving
//! `StreamAgentActivityDelta` in a live session room. It is not faked: see the crate README.

use std::path::Path;
use std::process::Command;

use pretty_assertions::assert_eq;
use tddy_service::proto::connection::{AgentActivityDeltaChunk, AgentActivityRecord, DeltaScope};
use tddy_service::proto::worktree_activity::{WorktreeActivityEvent, WorktreeActivityKind};
use tddy_session_sync::{
    decide_record, decide_worktree, delta_request, first_attach_commands, reassemble,
    reconcile_commands, Delta, DeltaError, GitInvocation, GitTransport, IgnoreReason,
    RecordDecision, SessionAddress, WorktreeDecision,
};
use tddy_session_sync::{DaemonToken, LOCAL_WIP_REF, REMOTE_GIT_SSH_COMMAND};

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

const SESSION_ID: &str = "1780828020298-abc";
const BASE_COMMIT: &str = "1111111111111111111111111111111111111111";

fn an_address() -> SessionAddress {
    SessionAddress {
        session_id: SESSION_ID.to_string(),
        project_id: "my-app".to_string(),
        worktree_path: "/home/dev/.tddy/worktrees/my-app/feat-x".to_string(),
        daemon_instance_id: "udoo-1780828020298".to_string(),
    }
}

/// An `Edit` the poll loop has already measured — the ordinary case a mirror acts on.
fn a_measured_record() -> AgentActivityRecord {
    AgentActivityRecord {
        call_id: "call-7".to_string(),
        tool_name: "Edit".to_string(),
        head_commit: BASE_COMMIT.to_string(),
        activity_seq: 7,
        changed_paths: vec!["src/main.rs".to_string()],
        ..Default::default()
    }
}

/// One frame of a delta stream. `total_byte_size` is the whole patch's, not this frame's — it
/// repeats on every frame, which is what makes a truncated stream detectable.
fn a_frame(patch: &[u8], total_byte_size: u64) -> AgentActivityDeltaChunk {
    AgentActivityDeltaChunk {
        patch: patch.to_vec(),
        seq: 7,
        prev_seq: 6,
        base_commit: BASE_COMMIT.to_string(),
        total_byte_size,
        scoped_paths: vec!["src/main.rs".to_string()],
    }
}

fn a_worktree_event(kind: WorktreeActivityKind) -> WorktreeActivityEvent {
    WorktreeActivityEvent {
        kind: kind as i32,
        seq: 4,
        head_commit: BASE_COMMIT.to_string(),
        ..Default::default()
    }
}

/// The transport a git subprocess is given: where the daemon is and who the syncer is.
fn a_transport() -> GitTransport {
    GitTransport {
        daemon_url: "http://udoo-1.example:8899".to_string(),
        token: DaemonToken::Refresh("refresh-token-value".to_string()),
    }
}

/// The argv of each command, which is what a test about command construction is about.
fn argv_of(commands: &[GitInvocation]) -> Vec<Vec<String>> {
    commands.iter().map(|c| c.args.clone()).collect()
}

/// The error a reassembly produced, for a test that is about the refusal rather than the patch.
fn rejected(chunks: &[AgentActivityDeltaChunk]) -> DeltaError {
    match reassemble(chunks) {
        Err(e) => e,
        Ok(delta) => panic!("expected the frames to be refused, got {delta:?}"),
    }
}

// ---------------------------------------------------------------------------
// What an activity record provokes
// ---------------------------------------------------------------------------

#[test]
fn fetches_the_tick_of_a_record_the_mirror_has_not_applied() {
    // Given a mirror that has applied tick 6 and a record measured at tick 7
    // When
    let decision = decide_record(&a_measured_record(), 6);

    // Then the fetch is addressed by the call that reported it.
    assert_eq!(
        decision,
        RecordDecision::FetchDelta {
            call_id: "call-7".to_string(),
            seq: 7,
        }
    );
}

#[test]
fn fetches_the_tick_of_a_record_whatever_tool_produced_it() {
    // Given a Bash call — a formatter, a codegen step — measured at a fresh tick
    let record = AgentActivityRecord {
        tool_name: "Bash".to_string(),
        changed_paths: Vec::new(),
        ..a_measured_record()
    };

    // When
    let decision = decide_record(&record, 6);

    // Then it is fetched like any other: a whitelist of editing tools is a list that goes out of
    // date, and a tool missing from it is a change that reaches the mirror never.
    assert_eq!(
        decision,
        RecordDecision::FetchDelta {
            call_id: "call-7".to_string(),
            seq: 7,
        }
    );
}

#[test]
fn ignores_a_record_no_poll_tick_has_measured_yet() {
    // Given a record recorded between two ticks, so its patch does not exist yet
    let record = AgentActivityRecord {
        activity_seq: 0,
        ..a_measured_record()
    };

    // When
    let decision = decide_record(&record, 6);

    // Then nothing is fetched — the tick that covers it reports it later.
    assert_eq!(decision, RecordDecision::Ignore(IgnoreReason::NoTickYet));
}

#[test]
fn ignores_the_second_record_of_a_poll_window_because_its_tick_is_applied() {
    // Given a mirror already at tick 7, and another of that window's calls arriving
    // When
    let decision = decide_record(&a_measured_record(), 7);

    // Then it is not fetched again: several calls share one tick and its patch applies once.
    assert_eq!(
        decision,
        RecordDecision::Ignore(IgnoreReason::AlreadyApplied {
            seq: 7,
            last_seq: 7,
        })
    );
}

#[test]
fn ignores_a_record_that_carries_no_call_id_to_address_a_delta_by() {
    // Given a record with no call id
    let record = AgentActivityRecord {
        call_id: String::new(),
        ..a_measured_record()
    };

    // When
    let decision = decide_record(&record, 6);

    // Then it is reported as unaddressable rather than fetched with an empty id, which would ask
    // the daemon for a call it cannot name.
    assert_eq!(
        decision,
        RecordDecision::Ignore(IgnoreReason::Unaddressable { seq: 7 })
    );
}

#[test]
fn asks_for_the_whole_tick_rather_than_the_calling_records_own_scope() {
    // Given the session and the call that reported the tick
    // When
    let request = delta_request(&an_address(), "access-token", "call-7");

    // Then the request is for the tick, because the mirror applies one patch per tick: a call's own
    // scope would deliver the first call of a poll window and silently drop every later one.
    assert_eq!(request.scope, DeltaScope::Tick as i32);
    assert_eq!(request.call_id, "call-7");
    assert_eq!(request.session_id, SESSION_ID);
    assert_eq!(request.daemon_instance_id, "udoo-1780828020298");
    assert_eq!(request.session_token, "access-token");
}

// ---------------------------------------------------------------------------
// Reassembling a delta stream
// ---------------------------------------------------------------------------

#[test]
fn reassembles_a_patch_split_across_frames() {
    // Given a patch delivered in two frames
    let frames = [
        a_frame(b"diff --git a/src", 34),
        a_frame(b"/main.rs b/src/mai", 34),
    ];

    // When
    let delta = reassemble(&frames).expect("must reassemble");

    // Then the frames concatenate in order, and the tick they describe is carried once.
    assert_eq!(
        delta,
        Delta {
            seq: 7,
            prev_seq: 6,
            base_commit: BASE_COMMIT.to_string(),
            patch: b"diff --git a/src/main.rs b/src/mai".to_vec(),
            scoped_paths: vec!["src/main.rs".to_string()],
        }
    );
}

#[test]
fn reads_a_call_that_changed_nothing_as_a_patch_of_no_bytes() {
    // Given the one empty frame a call that changed nothing is answered with (AC9)
    let frames = [a_frame(b"", 0)];

    // When
    let delta = reassemble(&frames).expect("must reassemble");

    // Then it is an empty patch at a known tick — not a failure, and not nothing.
    assert_eq!(delta.patch, Vec::<u8>::new());
    assert_eq!(delta.seq, 7);
}

#[test]
fn refuses_a_delta_stream_that_ended_without_a_single_frame() {
    // Given a stream that closed before it said anything
    // When
    let error = rejected(&[]);

    // Then it is a failure rather than an empty patch: "the call changed nothing" is one empty
    // frame, so reading a failed stream as that would skip a tick with nothing reporting it.
    assert_eq!(error, DeltaError::NoFrames);
}

#[test]
fn refuses_a_delta_whose_frames_disagree_about_the_commit_they_apply_onto() {
    // Given two frames describing different base commits
    let frames = [
        a_frame(b"diff", 8),
        AgentActivityDeltaChunk {
            base_commit: "2222222222222222222222222222222222222222".to_string(),
            ..a_frame(b"diff", 8)
        },
    ];

    // When
    let error = rejected(&frames);

    // Then the disagreement is reported rather than resolved by trusting the first frame — the
    // describing fields repeat on every frame precisely so this is detectable.
    assert_eq!(
        error,
        DeltaError::Inconsistent {
            field: "base_commit",
            first: BASE_COMMIT.to_string(),
            later: "2222222222222222222222222222222222222222".to_string(),
        }
    );
}

#[test]
fn refuses_a_delta_shorter_than_the_size_its_frames_declared() {
    // Given a stream that lost its second frame
    let frames = [a_frame(b"diff --git a/src", 34)];

    // When
    let error = rejected(&frames);

    // Then the truncation is named where the bytes went missing, rather than handed to `git apply`
    // as half a hunk.
    assert_eq!(
        error,
        DeltaError::Truncated {
            declared: 34,
            received: 16,
        }
    );
}

// ---------------------------------------------------------------------------
// What a worktree event provokes
// ---------------------------------------------------------------------------

#[test]
fn restores_from_git_when_the_session_commits() {
    // Given a commit event
    // When
    let decision = decide_worktree(&a_worktree_event(WorktreeActivityKind::Commit));

    // Then the mirror is restored from the WIP ref, whose parent is the new HEAD (AC28).
    assert_eq!(decision, WorktreeDecision::Reconcile);
}

#[test]
fn ignores_a_files_changed_event_because_the_ticks_delta_carries_its_content() {
    // Given a files-changed event, which by design carries no paths and no content
    // When
    let decision = decide_worktree(&a_worktree_event(WorktreeActivityKind::FilesChanged));

    // Then nothing is fetched for it: what changed arrives as the tick's delta.
    assert_eq!(decision, WorktreeDecision::Ignore);
}

// ---------------------------------------------------------------------------
// The git a first attach and a reconcile run
// ---------------------------------------------------------------------------

#[test]
fn checks_out_the_project_from_its_daemon_over_the_existing_git_transport() {
    // Given a session on a project of a named daemon
    // When
    let commands = first_attach_commands(&an_address(), &a_transport());

    // Then the repository is created in the destination the syncer already owns, pointed at
    // `{daemon}:{project}` — a registry name, never a filesystem path — and filled from the
    // session's WIP ref.
    assert_eq!(
        argv_of(&commands),
        vec![
            vec!["init".to_string(), "--quiet".to_string()],
            vec![
                "remote".to_string(),
                "add".to_string(),
                "origin".to_string(),
                "udoo-1780828020298:my-app".to_string()
            ],
            vec![
                "fetch".to_string(),
                "origin".to_string(),
                format!("+refs/tddy/session/{SESSION_ID}/wip:{LOCAL_WIP_REF}")
            ],
            vec![
                "reset".to_string(),
                "--hard".to_string(),
                format!("{LOCAL_WIP_REF}^")
            ],
            vec![
                "read-tree".to_string(),
                "-u".to_string(),
                "--reset".to_string(),
                LOCAL_WIP_REF.to_string()
            ],
        ]
    );
}

#[test]
fn reconciles_by_fetching_the_wip_ref_and_never_by_patching() {
    // Given a diverged mirror
    // When
    let commands = reconcile_commands(SESSION_ID, &a_transport());

    // Then recovery is a fetch and a reset onto it (AC31): git moves only the objects the mirror
    // is missing, and no whole-worktree patch exists to send.
    assert_eq!(
        argv_of(&commands),
        vec![
            vec![
                "fetch".to_string(),
                "origin".to_string(),
                format!("+refs/tddy/session/{SESSION_ID}/wip:{LOCAL_WIP_REF}")
            ],
            vec![
                "reset".to_string(),
                "--hard".to_string(),
                format!("{LOCAL_WIP_REF}^")
            ],
            vec![
                "read-tree".to_string(),
                "-u".to_string(),
                "--reset".to_string(),
                LOCAL_WIP_REF.to_string()
            ],
        ]
    );
}

#[test]
fn fetches_through_the_remote_git_shim_pointed_at_the_configured_daemon() {
    // Given a transport configured for one daemon
    // When
    let fetch = reconcile_commands(SESSION_ID, &a_transport()).remove(0);

    // Then git's transport is the shim, found on PATH, and it is told where the daemon is.
    assert_eq!(
        fetch.env,
        vec![
            (
                "GIT_SSH_COMMAND".to_string(),
                REMOTE_GIT_SSH_COMMAND.to_string()
            ),
            (
                "TDDY_DAEMON_URL".to_string(),
                "http://udoo-1.example:8899".to_string()
            ),
            (
                "TDDY_REFRESH_TOKEN".to_string(),
                "refresh-token-value".to_string()
            ),
        ]
    );
}

#[test]
fn hands_the_transport_an_access_token_when_that_is_what_the_syncer_was_given() {
    // Given a syncer configured with an access token rather than a refresh token
    let transport = GitTransport {
        token: DaemonToken::Access("access-token-value".to_string()),
        ..a_transport()
    };

    // When
    let fetch = reconcile_commands(SESSION_ID, &transport).remove(0);

    // Then it is passed on under the variable that names what it is — the shim exchanges a refresh
    // token itself and would reject an access token offered as one.
    assert_eq!(
        fetch.env.last().expect("the token is the last variable"),
        &(
            "TDDY_SESSION_TOKEN".to_string(),
            "access-token-value".to_string()
        )
    );
}

#[test]
fn never_renders_the_token_it_runs_git_with() {
    // Given a fetch carrying a daemon credential
    let fetch = reconcile_commands(SESSION_ID, &a_transport()).remove(0);

    // When it is rendered for a log line or a failure
    let rendered = fetch.command_line();

    // Then the command is named and the credential is not — a failed fetch is logged, and a log
    // that carries a 7-day refresh token is a credential in everyone's terminal scrollback.
    assert_eq!(
        rendered,
        format!("git fetch origin +refs/tddy/session/{SESSION_ID}/wip:{LOCAL_WIP_REF}")
    );
    assert!(
        !rendered.contains("refresh-token-value"),
        "a rendered command must not carry the token, got {rendered}"
    );
}

// ---------------------------------------------------------------------------
// The reconcile, run against real repositories
// ---------------------------------------------------------------------------

#[test]
fn restores_the_sessions_uncommitted_edits_while_leaving_head_on_the_sessions_commit() {
    // Given a session whose agent has written a file it has not committed, published as the
    // session's WIP ref, and a mirror pointed at that repository
    let scratch = tempfile::tempdir().expect("tempdir");
    let session = scratch.path().join("session");
    let mirror = scratch.path().join("mirror");
    let head = a_session_repo_with_uncommitted(&session, "draft.txt", "not committed yet\n");
    a_mirror_pointed_at(&mirror, &session);

    // When the reconcile the syncer would run is run
    run_all(&mirror, &reconcile_commands(SESSION_ID, &a_transport()));

    // Then the uncommitted file is in the mirror…
    assert_eq!(
        std::fs::read_to_string(mirror.join("draft.txt")).expect("the draft must be mirrored"),
        "not committed yet\n"
    );
    // …and HEAD is the session's own commit, not the WIP commit wrapping its tree. Every delta
    // that follows is cut from the session's HEAD, so a mirror parked on the WIP commit would
    // refuse all of them as base-commit mismatches and reconcile forever.
    assert_eq!(git(&mirror, &["rev-parse", "HEAD"]).trim(), head);
}

#[test]
fn discards_a_local_edit_made_inside_the_mirror() {
    // Given a mirror someone has edited by hand (AC33)
    let scratch = tempfile::tempdir().expect("tempdir");
    let session = scratch.path().join("session");
    let mirror = scratch.path().join("mirror");
    a_session_repo_with_uncommitted(&session, "draft.txt", "not committed yet\n");
    a_mirror_pointed_at(&mirror, &session);
    run_all(&mirror, &reconcile_commands(SESSION_ID, &a_transport()));
    std::fs::write(mirror.join("draft.txt"), "my own words\n").expect("edit the mirror");

    // When the next sync reconciles
    run_all(&mirror, &reconcile_commands(SESSION_ID, &a_transport()));

    // Then the local edit is gone without a prompt: the directory is a mirror, not a workspace.
    assert_eq!(
        std::fs::read_to_string(mirror.join("draft.txt")).expect("the draft must be mirrored"),
        "not committed yet\n"
    );
}

/// A session's repository: one commit, one uncommitted file, and the WIP ref the daemon publishes
/// each tick — a commit wrapping the `git add -A` tree, parented on `HEAD`. Returns the commit the
/// session is on, which is what the mirror must end up on too.
fn a_session_repo_with_uncommitted(root: &Path, path: &str, contents: &str) -> String {
    std::fs::create_dir_all(root).expect("create the session repo");
    git(root, &["init", "--quiet", "--initial-branch=main"]);
    git(root, &["config", "user.email", "agent@example.com"]);
    git(root, &["config", "user.name", "Agent"]);
    std::fs::write(root.join("README.md"), "one\n").expect("write README");
    git(root, &["add", "."]);
    git(root, &["commit", "--quiet", "-m", "initial"]);
    let head = git(root, &["rev-parse", "HEAD"]).trim().to_string();

    std::fs::write(root.join(path), contents).expect("write the uncommitted file");
    // A temporary index, exactly as the daemon's poll loop uses one: `git add -A` against the
    // agent's real index would rewrite the staging area out from under it.
    let index = root.join(".tddy-wip-index");
    let index = index.to_str().expect("a utf-8 temp index path");
    git_with_index(root, index, &["add", "-A"]);
    let wip_tree = git_with_index(root, index, &["write-tree"])
        .trim()
        .to_string();
    let wip = git(root, &["commit-tree", &wip_tree, "-p", &head, "-m", "wip"])
        .trim()
        .to_string();
    git(
        root,
        &[
            "update-ref",
            &format!("refs/tddy/session/{SESSION_ID}/wip"),
            &wip,
        ],
    );
    head
}

/// A destination with a repository pointed at the session's, which is what the first two commands
/// of a first attach produce.
fn a_mirror_pointed_at(root: &Path, session: &Path) {
    std::fs::create_dir_all(root).expect("create the mirror");
    git(root, &["init", "--quiet"]);
    git(
        root,
        &[
            "remote",
            "add",
            "origin",
            session.to_str().expect("a utf-8 session path"),
        ],
    );
}

/// Run the syncer's own command list, argv for argv. The environment is not applied: these run
/// against a filesystem remote, which needs no transport at all.
fn run_all(cwd: &Path, commands: &[GitInvocation]) {
    for command in commands {
        let args: Vec<&str> = command.args.iter().map(String::as_str).collect();
        git(cwd, &args);
    }
}

/// Run git in `cwd`, returning stdout. A failure panics with git's own stderr — a test that hid it
/// would report "the mirror is wrong" when the truth is "the fixture never built".
fn git(cwd: &Path, args: &[&str]) -> String {
    run_git_process(Command::new("git").args(args).current_dir(cwd), args)
}

fn git_with_index(cwd: &Path, index: &str, args: &[&str]) -> String {
    run_git_process(
        Command::new("git")
            .args(args)
            .current_dir(cwd)
            .env("GIT_INDEX_FILE", index),
        args,
    )
}

fn run_git_process(command: &mut Command, args: &[&str]) -> String {
    let output = command
        .output()
        .unwrap_or_else(|e| panic!("failed to run git {args:?}: {e}"));
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}
