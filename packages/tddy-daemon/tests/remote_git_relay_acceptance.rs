//! Acceptance: the git child relay — byte fidelity, stream separation, exit status, teardown.
//!
//! PRD: docs/ft/daemon/remote-git-repo.md § Server — byte fidelity and lifecycle (AC13, AC15–AC19).
//!
//! This is the reason the feature does not reuse the terminal RPC. A pack stream must survive the
//! round trip **bit for bit**: no PTY line discipline rewriting `\n`, no prologue prepended, no
//! frame dropped by a lagging broadcast, stderr kept out of the pack, and the child's exit status
//! reported. The later tests pin the other deliberate differences — a git process does not outlive
//! the connection that asked for it, it never inherits the daemon's own environment, and neither
//! its output nor the number of children the daemon will run is allowed to grow without bound.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use pretty_assertions::assert_eq;
use tddy_daemon::pty_runtime::ResolvedPtyUser;
use tddy_daemon::remote_git_service::{
    git_argv, git_argv_as_user, git_child_command, GitChildRelay, GitStreamSlots, GitVerb,
    GIT_FRAME_CHANNEL_CAPACITY, MAX_GIT_FRAME_BYTES,
};
use tddy_rpc::Code;
use tddy_service::proto::remote_git::GitServerFrame;
use tokio::sync::mpsc::Receiver;

/// Everything a completed relay emitted, reassembled in arrival order.
struct RelayedStreams {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    exit_code: i32,
    frames: Vec<GitServerFrame>,
}

impl RelayedStreams {
    fn largest_frame_payload(&self) -> usize {
        self.frames
            .iter()
            .map(|f| f.stdout.len().max(f.stderr.len()))
            .max()
            .unwrap_or(0)
    }

    fn done_frame_count(&self) -> usize {
        self.frames.iter().filter(|f| f.done).count()
    }

    fn last_frame_is_the_done_frame(&self) -> bool {
        self.frames.last().is_some_and(|f| f.done)
    }
}

/// Drain a relay's frames until the stream closes.
async fn drained(mut rx: Receiver<Result<GitServerFrame, tddy_rpc::Status>>) -> RelayedStreams {
    let mut streams = RelayedStreams {
        stdout: Vec::new(),
        stderr: Vec::new(),
        exit_code: i32::MIN,
        frames: Vec::new(),
    };
    while let Ok(Some(frame)) = tokio::time::timeout(Duration::from_secs(20), rx.recv()).await {
        let frame = frame.expect("the relay must not fault mid-stream");
        streams.stdout.extend_from_slice(&frame.stdout);
        streams.stderr.extend_from_slice(&frame.stderr);
        if frame.done {
            streams.exit_code = frame.exit_code;
        }
        streams.frames.push(frame);
    }
    streams
}

fn a_shell_child(script: &str) -> Vec<String> {
    vec!["/bin/sh".to_string(), "-c".to_string(), script.to_string()]
}

/// A payload whose every byte is derivable from its offset, so a lost or reordered one shows up.
fn checkable_bytes(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 251) as u8).collect()
}

/// A child that writes `payload` to stdout and nothing else, with no shell in the way.
fn a_child_emitting(payload: &[u8], cwd: &Path) -> Vec<String> {
    let source = cwd.join("payload.bin");
    std::fs::write(&source, payload).expect("write the child's source file");
    vec![
        "/bin/cat".to_string(),
        source.to_string_lossy().into_owned(),
    ]
}

fn a_working_directory() -> tempfile::TempDir {
    tempfile::tempdir().expect("tempdir")
}

#[cfg(target_os = "linux")]
fn process_is_gone(pid: u32) -> bool {
    !Path::new(&format!("/proc/{pid}")).exists()
}

#[cfg(not(target_os = "linux"))]
fn process_is_gone(pid: u32) -> bool {
    !std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .expect("kill -0 must run")
        .success()
}

/// Wait for a child to publish a pid, then read it.
async fn pid_written_to(path: &Path) -> u32 {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if let Ok(contents) = std::fs::read_to_string(path) {
            if let Ok(pid) = contents.trim().parse::<u32>() {
                return pid;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!(
        "the child never published its worker pid to {}",
        path.display()
    );
}

async fn wait_until_gone(pid: u32, within: Duration) -> bool {
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        if process_is_gone(pid) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    process_is_gone(pid)
}

// --- AC13: the argv the child is spawned with -------------------------------------------------

#[test]
fn spawns_the_pack_verb_as_a_git_subcommand_against_the_registry_resolved_repository() {
    // Given a project checkout the registry resolved to
    let repo = PathBuf::from("/home/dev/repos/my-app");

    // When
    let argv = git_argv(GitVerb::UploadPack, &repo, None).expect("argv must build");

    // Then
    assert_eq!(
        argv,
        vec![
            "git".to_string(),
            "upload-pack".to_string(),
            "--".to_string(),
            "/home/dev/repos/my-app".to_string()
        ]
    );
}

#[test]
fn separates_the_repository_path_from_the_options_git_would_otherwise_parse_it_as() {
    // Given a `main_repo_path` beginning with a dash — `projects.yaml` is hand-editable
    let repo = PathBuf::from("--upload-pack=/usr/bin/id");

    // When
    let argv = git_argv(GitVerb::UploadPack, &repo, None).expect("argv must build");

    // Then the path arrives after an end-of-options marker, so git reads it as a path
    assert_eq!(
        argv,
        vec![
            "git".to_string(),
            "upload-pack".to_string(),
            "--".to_string(),
            "--upload-pack=/usr/bin/id".to_string()
        ]
    );
}

#[test]
fn spawns_receive_pack_for_a_push() {
    // Given a push against the same checkout
    let repo = PathBuf::from("/home/dev/repos/my-app");

    // When
    let argv = git_argv(GitVerb::ReceivePack, &repo, None).expect("argv must build");

    // Then
    assert_eq!(argv[..2], ["git".to_string(), "receive-pack".to_string()]);
}

#[test]
fn never_routes_the_child_through_a_shell_so_a_repository_path_cannot_be_interpreted() {
    // Given a checkout path containing characters a shell would act on
    let repo = PathBuf::from("/home/dev/repos/my-app; id");

    // When
    let argv = git_argv(GitVerb::UploadPack, &repo, None).expect("argv must build");

    // Then the path is one argument, and no shell is in the argv to interpret it
    assert_eq!(
        argv.last().expect("a repo argument"),
        "/home/dev/repos/my-app; id"
    );
    assert!(
        !argv
            .iter()
            .any(|a| a == "-c" || a.ends_with("/sh") || a.ends_with("/bash")),
        "argv must exec git directly, got: {argv:?}"
    );
}

#[test]
fn does_not_wrap_the_child_when_the_target_user_is_the_daemons_own_identity() {
    // Given the daemon serving a project owned by the user it already runs as
    let repo = PathBuf::from("/home/dev/repos/my-app");
    let own_user = std::env::var("USER").expect("USER must be set");

    // When
    let argv = git_argv(GitVerb::UploadPack, &repo, Some(&own_user)).expect("argv must build");

    // Then there is nothing to drop to, so no setpriv wrapper is added
    assert_eq!(argv.first().expect("a command"), "git");
}

/// The uid/gid of an account the test host does not have to own — the passwd lookup is the only
/// part of the resolution that needs a real one.
fn a_project_owner(uid: u32, gid: u32) -> ResolvedPtyUser {
    ResolvedPtyUser {
        uid,
        gid,
        home_dir: "/home/project-owner".to_string(),
    }
}

#[test]
fn drops_privilege_to_the_projects_own_os_user_before_execing_git() {
    // Given a root daemon serving a project owned by a different OS user
    let repo = PathBuf::from("/home/dev/repos/my-app");
    let owner = a_project_owner(4242, 4343);

    // When
    let argv = git_argv_as_user(GitVerb::UploadPack, &repo, &owner, 0, 0);

    // Then git execs behind a setpriv that has already dropped to the project's user
    assert_eq!(
        argv,
        vec![
            "setpriv",
            "--reuid",
            "4242",
            "--regid",
            "4343",
            "--init-groups",
            "--",
            "git",
            "upload-pack",
            "--",
            "/home/dev/repos/my-app",
        ]
    );
}

#[test]
fn execs_git_directly_when_the_project_owner_is_already_the_daemons_identity() {
    // Given a daemon already running as the project's owner (dev / single-user)
    let repo = PathBuf::from("/home/dev/repos/my-app");
    let owner = a_project_owner(1000, 1000);

    // When
    let argv = git_argv_as_user(GitVerb::UploadPack, &repo, &owner, 1000, 1000);

    // Then there is nothing to drop to, so no setuid launcher is inserted
    assert_eq!(
        argv,
        vec!["git", "upload-pack", "--", "/home/dev/repos/my-app"]
    );
}

#[test]
fn drops_privilege_when_only_the_group_differs_from_the_daemons_identity() {
    // Given a project whose owner shares the daemon's uid but not its primary group
    let repo = PathBuf::from("/home/dev/repos/my-app");
    let owner = a_project_owner(1000, 2000);

    // When
    let argv = git_argv_as_user(GitVerb::UploadPack, &repo, &owner, 1000, 1000);

    // Then the child still execs behind setpriv, so it cannot keep the daemon's group
    assert_eq!(argv.first().expect("a command"), "setpriv");
}

// --- AC13: the environment the child runs with -------------------------------------------------

#[test]
fn gives_the_git_child_the_project_owners_home_rather_than_the_daemons() {
    // Given a project owned by an account that exists in the passwd database
    let own_user = std::env::var("USER").expect("USER must be set");
    let own_home = std::env::var("HOME").expect("HOME must be set");

    // When the daemon resolves the command for it
    let command = git_child_command(
        GitVerb::UploadPack,
        Path::new("/home/dev/repos/my-app"),
        &own_user,
    )
    .expect("the command must resolve");

    // Then git reads the owner's `.gitconfig`, not whichever one the daemon's HOME points at
    let home = command
        .env
        .iter()
        .find(|(key, _)| key == "HOME")
        .map(|(_, value)| value.as_str());
    assert_eq!(home, Some(own_home.as_str()));
}

#[tokio::test]
async fn gives_a_child_only_the_environment_it_was_spawned_with() {
    // Given a child spawned with one variable, by a daemon whose own environment holds the LiveKit
    // API secret it signs session tokens with
    let cwd = a_working_directory();
    let (relay, rx) = GitChildRelay::spawn_with_env(
        vec!["/usr/bin/env".to_string()],
        cwd.path().to_path_buf(),
        vec![("HOME".to_string(), "/home/project-owner".to_string())],
    )
    .expect("env must spawn");

    // When the child reports the environment it actually received
    relay.close_stdin().await.expect("stdin must close");
    let streams = drained(rx).await;

    // Then nothing of the daemon's crosses the uid boundary `setpriv` would otherwise carry it
    // over — `git receive-pack` runs repository hooks, which would read it
    assert_eq!(
        String::from_utf8_lossy(&streams.stdout),
        "HOME=/home/project-owner\n"
    );
}

// --- AC13: the working directory the child runs in ---------------------------------------------

#[tokio::test]
async fn runs_the_child_in_the_repository_directory_it_was_given() {
    // Given a directory the child can only name by having been started in it
    let cwd = a_working_directory();
    let expected = std::fs::canonicalize(cwd.path()).expect("the working directory must resolve");
    let (relay, rx) = GitChildRelay::spawn_under_daemon_identity(
        vec!["/bin/pwd".to_string(), "-P".to_string()],
        cwd.path().to_path_buf(),
    )
    .expect("pwd must spawn");

    // When
    relay.close_stdin().await.expect("stdin must close");
    let streams = drained(rx).await;

    // Then it is the registry's `main_repo_path`, which is what git resolves hooks, alternates and
    // the worktree against — the repository argument alone would leave those pointing elsewhere
    assert_eq!(
        String::from_utf8_lossy(&streams.stdout).trim_end(),
        expected.to_string_lossy()
    );
}

// --- AC15: byte fidelity ----------------------------------------------------------------------

#[tokio::test]
async fn relays_stdin_to_the_child_and_its_stdout_back_byte_for_byte() {
    // Given a child that echoes its stdin, and a payload with bytes a text channel would mangle
    let cwd = a_working_directory();
    let payload: Vec<u8> = vec![
        0x00, 0x01, b'a', 0x0a, b'b', 0x0d, 0xff, 0xfe, 0x1b, b'[', b'?', b'1', b'h',
    ];
    let (relay, rx) = GitChildRelay::spawn_under_daemon_identity(
        vec!["/bin/cat".to_string()],
        cwd.path().to_path_buf(),
    )
    .expect("cat must spawn");

    // When
    relay
        .send_stdin(payload.clone())
        .await
        .expect("stdin must be written");
    relay.close_stdin().await.expect("stdin must close");
    let streams = drained(rx).await;

    // Then
    assert_eq!(streams.stdout, payload);
}

#[tokio::test]
async fn does_not_translate_a_newline_the_way_a_pty_line_discipline_would() {
    // Given the exact corruption that rules out serving git over the terminal RPC
    let cwd = a_working_directory();
    let (relay, rx) = GitChildRelay::spawn_under_daemon_identity(
        vec!["/bin/cat".to_string()],
        cwd.path().to_path_buf(),
    )
    .expect("cat must spawn");

    // When
    relay
        .send_stdin(b"a\nb\nc".to_vec())
        .await
        .expect("stdin must be written");
    relay.close_stdin().await.expect("stdin must close");
    let streams = drained(rx).await;

    // Then no carriage returns appear — a PTY would have produced "a\r\nb\r\nc"
    assert_eq!(streams.stdout, b"a\nb\nc".to_vec());
}

#[tokio::test]
async fn emits_nothing_before_the_childs_first_byte() {
    // Given a child whose entire output is a single known byte
    let cwd = a_working_directory();
    let (relay, rx) = GitChildRelay::spawn_under_daemon_identity(
        a_shell_child("printf X"),
        cwd.path().to_path_buf(),
    )
    .expect("child must spawn");

    // When
    relay.close_stdin().await.expect("stdin must close");
    let streams = drained(rx).await;

    // Then there is no prologue — the terminal bridge prepends a mode-setting escape here
    assert_eq!(streams.stdout, b"X".to_vec());
}

#[tokio::test]
async fn carries_a_payload_larger_than_one_frame_without_losing_or_reordering_a_byte() {
    // Given a payload several frames long, whose every byte is checkable
    let cwd = a_working_directory();
    let payload = checkable_bytes(MAX_GIT_FRAME_BYTES * 3 + 517);
    let (relay, rx) = GitChildRelay::spawn_under_daemon_identity(
        vec!["/bin/cat".to_string()],
        cwd.path().to_path_buf(),
    )
    .expect("cat must spawn");

    // When
    relay
        .send_stdin(payload.clone())
        .await
        .expect("stdin must be written");
    relay.close_stdin().await.expect("stdin must close");
    let streams = drained(rx).await;

    // Then
    assert_eq!(streams.stdout.len(), payload.len());
    assert_eq!(streams.stdout, payload);
}

// --- AC16: stderr is its own stream -----------------------------------------------------------

#[tokio::test]
async fn keeps_the_childs_stderr_out_of_the_stdout_byte_stream() {
    // Given a child that writes progress to stderr while writing a pack to stdout — what git does
    let cwd = a_working_directory();
    let (relay, rx) = GitChildRelay::spawn_under_daemon_identity(
        a_shell_child("printf PACK >&1; printf 'Counting objects' >&2; printf DATA >&1"),
        cwd.path().to_path_buf(),
    )
    .expect("child must spawn");

    // When
    relay.close_stdin().await.expect("stdin must close");
    let streams = drained(rx).await;

    // Then
    assert_eq!(streams.stdout, b"PACKDATA".to_vec());
    assert_eq!(streams.stderr, b"Counting objects".to_vec());
}

// --- AC17: frame size -------------------------------------------------------------------------

#[tokio::test]
async fn chunks_output_so_no_frame_reaches_the_transports_per_message_limit() {
    // Given far more output than one frame can carry
    let cwd = a_working_directory();
    let (relay, rx) = GitChildRelay::spawn_under_daemon_identity(
        a_shell_child(&format!("head -c {} /dev/zero", MAX_GIT_FRAME_BYTES * 4)),
        cwd.path().to_path_buf(),
    )
    .expect("child must spawn");

    // When
    relay.close_stdin().await.expect("stdin must close");
    let streams = drained(rx).await;

    // Then every frame stays under the chunking threshold, so none is ever chunk-framed
    assert_eq!(streams.stdout.len(), MAX_GIT_FRAME_BYTES * 4);
    assert!(
        streams.largest_frame_payload() <= MAX_GIT_FRAME_BYTES,
        "largest frame was {} bytes, limit is {MAX_GIT_FRAME_BYTES}",
        streams.largest_frame_payload()
    );
}

// Both operands are constants on purpose — the relation between them is the invariant under test.
#[allow(clippy::assertions_on_constants)]
#[test]
fn keeps_the_frame_budget_below_the_livekit_chunking_threshold() {
    // Given the transport's per-message limit, above which a payload is split into chunk frames
    // and a single lost frame wedges the call with no error
    // When / Then
    assert!(
        MAX_GIT_FRAME_BYTES < tddy_livekit::chunking::MAX_CHUNK_FRAME_BYTES,
        "a git frame must never need chunk framing"
    );
}

// --- Flow control: a pack must not accumulate in the daemon's heap ----------------------------

/// How long a child is left to run ahead of a reader that is not draining. Half a second is far
/// more than an unbounded relay would need to swallow the whole file.
const RUN_AHEAD_WINDOW: Duration = Duration::from_millis(500);

#[tokio::test]
async fn buffers_no_more_than_the_frame_channels_capacity_while_the_reader_is_behind() {
    // Given a child whose output dwarfs the frame channel — every real pack does
    let cwd = a_working_directory();
    let argv = a_child_emitting(&checkable_bytes(MAX_GIT_FRAME_BYTES * 64), cwd.path());
    let (_relay, rx) = GitChildRelay::spawn_under_daemon_identity(argv, cwd.path().to_path_buf())
        .expect("cat must spawn");

    // When nothing reads the frames for long enough that an unbounded relay would have drained the
    // child completely
    tokio::time::sleep(RUN_AHEAD_WINDOW).await;

    // Then the daemon holds the channel's capacity and no more. A 2 GB clone costs 2 GB of wire,
    // not 2 GB of daemon heap, so concurrent clones cannot take the host's sessions down with it.
    assert_eq!(rx.len(), GIT_FRAME_CHANNEL_CAPACITY);
}

#[tokio::test]
async fn delivers_every_byte_in_order_to_a_reader_that_falls_behind_the_child() {
    // Given the same oversized output
    let cwd = a_working_directory();
    let payload = checkable_bytes(MAX_GIT_FRAME_BYTES * 64);
    let argv = a_child_emitting(&payload, cwd.path());
    let (_relay, rx) = GitChildRelay::spawn_under_daemon_identity(argv, cwd.path().to_path_buf())
        .expect("cat must spawn");

    // When the reader only starts once the child has already run the channel full
    tokio::time::sleep(RUN_AHEAD_WINDOW).await;
    let streams = drained(rx).await;

    // Then holding the pump back cost nothing: no byte was dropped and none arrived out of order
    assert_eq!(streams.stdout.len(), payload.len());
    assert_eq!(streams.stdout, payload);
}

// --- AC18: exit status ------------------------------------------------------------------------

#[tokio::test]
async fn reports_a_successful_childs_exit_status_in_a_final_done_frame() {
    // Given a child that succeeds
    let cwd = a_working_directory();
    let (relay, rx) = GitChildRelay::spawn_under_daemon_identity(
        a_shell_child("exit 0"),
        cwd.path().to_path_buf(),
    )
    .expect("child must spawn");

    // When
    relay.close_stdin().await.expect("stdin must close");
    let streams = drained(rx).await;

    // Then
    assert_eq!(streams.exit_code, 0);
    assert_eq!(streams.done_frame_count(), 1);
}

#[tokio::test]
async fn reports_a_failing_childs_exit_status_so_git_sees_the_true_remote_result() {
    // Given a child that fails the way `git-receive-pack` fails a rejected push
    let cwd = a_working_directory();
    let (relay, rx) = GitChildRelay::spawn_under_daemon_identity(
        a_shell_child("exit 3"),
        cwd.path().to_path_buf(),
    )
    .expect("child must spawn");

    // When
    relay.close_stdin().await.expect("stdin must close");
    let streams = drained(rx).await;

    // Then
    assert_eq!(streams.exit_code, 3);
}

#[tokio::test]
async fn emits_the_done_frame_last_and_after_every_output_byte() {
    // Given a child that writes output and then exits non-zero
    let cwd = a_working_directory();
    let (relay, rx) = GitChildRelay::spawn_under_daemon_identity(
        a_shell_child("printf 'the last bytes'; exit 7"),
        cwd.path().to_path_buf(),
    )
    .expect("child must spawn");

    // When
    relay.close_stdin().await.expect("stdin must close");
    let streams = drained(rx).await;

    // Then output that arrived before the exit is not lost to the teardown
    assert_eq!(streams.stdout, b"the last bytes".to_vec());
    assert!(
        streams.last_frame_is_the_done_frame(),
        "the done frame must terminate the stream"
    );
    assert_eq!(streams.exit_code, 7);
}

#[tokio::test]
async fn closing_stdin_lets_a_child_that_reads_to_end_of_input_finish() {
    // Given a child that blocks until its stdin reaches EOF — `git-upload-pack`'s negotiation
    let cwd = a_working_directory();
    let (relay, rx) = GitChildRelay::spawn_under_daemon_identity(
        a_shell_child("cat > /dev/null; printf done"),
        cwd.path().to_path_buf(),
    )
    .expect("child must spawn");

    // When
    relay
        .send_stdin(b"0000".to_vec())
        .await
        .expect("stdin must be written");
    relay.close_stdin().await.expect("stdin must close");
    let streams = drained(rx).await;

    // Then
    assert_eq!(streams.stdout, b"done".to_vec());
    assert_eq!(streams.exit_code, 0);
}

/// Deliberately slow: the relay gives a stalled output pipe a fixed window to reach EOF before it
/// kills the process group, and this test exists to wait that window out. Nothing shorter proves
/// the stream is closed rather than merely slow.
#[tokio::test]
async fn reports_the_exit_status_even_when_a_forked_worker_still_holds_the_output_pipe() {
    // Given a child that exits while a worker it forked keeps its stdout open — what
    // `git-upload-pack` does when `pack-objects` outlives it
    let cwd = a_working_directory();
    let (relay, rx) = GitChildRelay::spawn_under_daemon_identity(
        a_shell_child("sleep 300 &"),
        cwd.path().to_path_buf(),
    )
    .expect("child must spawn");

    // When
    relay.close_stdin().await.expect("stdin must close");
    let streams = drained(rx).await;

    // Then the client is told how the child ended, instead of waiting forever on a done frame that
    // an unbounded wait for EOF would never produce
    assert_eq!(streams.exit_code, 0);
    assert_eq!(streams.done_frame_count(), 1);
}

// --- AC19: the child does not outlive the connection ------------------------------------------

#[tokio::test]
async fn terminates_the_git_child_when_the_relay_is_dropped() {
    // Given a long-running child and a connection that goes away mid-operation
    let cwd = a_working_directory();
    let (relay, _rx) = GitChildRelay::spawn_under_daemon_identity(
        a_shell_child("sleep 300"),
        cwd.path().to_path_buf(),
    )
    .expect("child must spawn");
    let pid = relay.pid();
    assert!(
        !process_is_gone(pid),
        "the child must be running to start with"
    );

    // When the connection ends
    drop(relay);

    // Then the child is signalled and reaped — unlike the terminal path, which leaves its PTY
    // running on disconnect by design
    assert!(
        wait_until_gone(pid, Duration::from_secs(10)).await,
        "git child {pid} outlived the connection that asked for it"
    );
}

#[tokio::test]
async fn terminates_a_worker_the_git_process_forked_for_itself() {
    // Given a child that forks a worker — which is exactly what `git-upload-pack` does when it
    // spawns `pack-objects`
    let cwd = a_working_directory();
    let pid_file = cwd.path().join("worker.pid");
    let (relay, _rx) = GitChildRelay::spawn_under_daemon_identity(
        a_shell_child(&format!(
            "sleep 300 & echo $! > {}; wait",
            pid_file.display()
        )),
        cwd.path().to_path_buf(),
    )
    .expect("child must spawn");
    let worker_pid = pid_written_to(&pid_file).await;

    // When the connection ends
    drop(relay);

    // Then the whole process group goes. A surviving worker would hold the repository's object
    // database open for as long as it ran, invisible to everything that manages the daemon.
    assert!(
        wait_until_gone(worker_pid, Duration::from_secs(15)).await,
        "forked worker {worker_pid} outlived the connection"
    );
}

#[tokio::test]
async fn terminates_a_child_that_ignores_a_polite_signal() {
    // Given a child that traps SIGTERM, as a wedged process would
    let cwd = a_working_directory();
    let (relay, _rx) = GitChildRelay::spawn_under_daemon_identity(
        a_shell_child("trap '' TERM; sleep 300"),
        cwd.path().to_path_buf(),
    )
    .expect("child must spawn");
    let pid = relay.pid();

    // When
    drop(relay);

    // Then the grace period expires and it is killed outright
    assert!(
        wait_until_gone(pid, Duration::from_secs(15)).await,
        "a child ignoring SIGTERM must still be killed, pid {pid}"
    );
}

// --- Concurrency: one client must not be able to exhaust the host ------------------------------

#[test]
fn refuses_a_new_stream_once_every_concurrent_slot_is_taken() {
    // Given a daemon willing to run two git children, both already running
    let slots = GitStreamSlots::new(2);
    let _first = slots.acquire().expect("the first slot must be free");
    let _second = slots.acquire().expect("the second slot must be free");

    // When a third client opens a stream
    let status = slots.acquire().expect_err("a third stream must be refused");

    // Then it is told the host is full, rather than the daemon forking until the process table is
    assert_eq!(status.code(), Code::ResourceExhausted);
}

#[test]
fn readmits_a_client_once_a_finished_stream_gives_its_slot_back() {
    // Given the only slot, taken
    let slots = GitStreamSlots::new(1);
    let held = slots.acquire().expect("the only slot must be free");

    // When the stream holding it ends
    drop(held);

    // Then the next client is admitted
    slots
        .acquire()
        .expect("a slot a finished stream gave back must be reusable");
}
