//! `remote_git.RemoteGitService` — serve a daemon **project** as a git remote.
//!
//! One `Serve` stream carries one git pack operation. The first client frame authorizes it
//! (`session_token` → GitHub user → OS user), names the project (by id or name, resolved against
//! *that user's own* `~/.tddy/projects/projects.yaml`) and names the verb. The daemon then spawns
//! the real `git-upload-pack` / `git-receive-pack` on **pipes** — never a PTY — as that OS user,
//! relays bytes verbatim in both directions, and reports the child's exit status in a final frame.
//!
//! Two properties keep this from being a remote shell:
//!
//! - The verb whitelist is closed and enforced here, not on the client.
//! - The repository path comes from the project registry, never from the request, so no
//!   `project_ref` can select a directory the registry does not already name.
//!
//! Unlike the terminal RPCs, **the child does not outlive the connection**: dropping the relay
//! signals the child. See docs/ft/daemon/remote-git-repo.md.
//!
//! Three resources are bounded rather than left to the workload: the child's environment (it is
//! built here, never inherited, because `setpriv` would otherwise carry the daemon's secrets across
//! the uid boundary), the output the relay will hold before it stops reading the child, and the
//! number of git children the daemon will run at once.

use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot, Mutex, OwnedSemaphorePermit, Semaphore};

use tddy_rpc::{Code, Status};
use tddy_service::proto::remote_git::{GitClientFrame, GitOpen, GitServerFrame};

use crate::config::DaemonConfig;
use crate::pty_runtime::ResolvedPtyUser;

/// The log target every line in this module carries, so an operator can follow one clone through
/// the journal.
const LOG_TARGET: &str = "tddy_daemon::remote_git_service";

/// Resolves a `session_token` to a GitHub login. The daemon's own resolver
/// ([`crate::auth`]) verifies the HMAC signature, the expiry, and that the token is access-kind.
pub type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// Resolves an OS user to that user's project registry directory
/// (`~/.tddy/projects/`). Mirrors `ConnectionServiceImpl`'s sessions-base resolver.
pub type ProjectsDirResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;

/// Largest payload carried in one `GitServerFrame`. Kept well under
/// `tddy_livekit::chunking::MAX_CHUNK_FRAME_BYTES` (60 000) so a git frame is never chunk-framed —
/// a lost chunk wedges the call with no error, and a pack stream is exactly the workload that
/// would produce oversized frames.
pub const MAX_GIT_FRAME_BYTES: usize = 32 * 1024;

/// How many output frames a relay may hold before its pumps stop reading the child.
///
/// The channel is **bounded** on purpose: every consumer downstream of it is, so an unbounded one
/// would let a `pack-objects` that outruns the wire accumulate the whole pack in the daemon's heap
/// — a 2 GB clone becoming 2 GB of RSS, and a handful of concurrent clones taking every session on
/// the host down with the daemon. Blocking the pump instead pushes the backpressure into the
/// kernel pipe buffer and then into the child's own `write`, which is where git already handles it.
///
/// Eight costs nothing in speed: a 150 MiB clone measures 2.54 MiB/s at this capacity and 2.54
/// MiB/s at 64, so the transfer is bound by the LiveKit data channel rather than by how much the
/// daemon is willing to hold. Raising it would buy latency headroom the wire cannot use, in exchange
/// for the memory ceiling above. Re-measure with
/// `clones_a_large_repository_with_every_byte_intact`.
pub const GIT_FRAME_CHANNEL_CAPACITY: usize = 8;

/// How many git children this daemon will run at once, across every user.
pub const MAX_CONCURRENT_GIT_STREAMS: usize = 16;

/// How long a signalled child is given to exit on its own before it is killed outright. A
/// `git-upload-pack` that has already streamed a pack needs no more than a moment to unwind; a
/// process that ignores `SIGTERM` must not keep the repository's object database open for longer.
const CHILD_TERMINATION_GRACE: Duration = Duration::from_secs(3);

/// How long the output pumps may make **no progress at all** after the child itself has exited,
/// before the process group is signalled so the stream can be closed.
///
/// `git-upload-pack` forks `pack-objects`, which inherits stdout. If such a grandchild outlives the
/// process the daemon spawned, the pipe never reaches EOF, and waiting on it forever would leave
/// the client hanging on a `done` frame that never comes and an error that is never reported. The
/// window is only ever entered when the pumps are neither delivering frames nor parked writing one,
/// so a slow wire cannot trip it.
const PUMP_STALL_TIMEOUT: Duration = Duration::from_secs(10);

/// The git pack verbs this service will run. A closed set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitVerb {
    UploadPack,
    ReceivePack,
}

impl GitVerb {
    /// The git subcommand pair this verb execs, e.g. `("git", "upload-pack")`.
    pub fn git_subcommand(self) -> &'static str {
        match self {
            GitVerb::UploadPack => "upload-pack",
            GitVerb::ReceivePack => "receive-pack",
        }
    }
}

/// Read the `open` payload from a `Serve` stream's first frame.
///
/// Only `open` is read here. The frame's `stdin`/`stdin_eof` are the caller's to relay — `serve`
/// takes them before calling this and applies them to the child once it exists, so a client that
/// sends payload alongside its open loses nothing.
pub fn open_from_first_frame(frame: GitClientFrame) -> Result<GitOpen, Status> {
    frame.open.ok_or_else(|| {
        Status::invalid_argument("the first frame of a Serve stream must carry `open`")
    })
}

/// Resolve a wire `verb` against the closed whitelist. Accepts both spellings git uses
/// (`git-upload-pack` and `git upload-pack`).
pub fn resolve_git_verb(verb: &str) -> Result<GitVerb, Status> {
    match verb {
        "git-upload-pack" | "git upload-pack" => Ok(GitVerb::UploadPack),
        "git-receive-pack" | "git receive-pack" => Ok(GitVerb::ReceivePack),
        other => Err(Status::permission_denied(format!(
            "'{other}' is not a git pack verb; only git-upload-pack and git-receive-pack are served"
        ))),
    }
}

/// Resolve `project_ref` to the project's `main_repo_path`, matching `project_id` first and `name`
/// second. The returned path always comes from the registry — never from `project_ref`.
pub fn resolve_project_repo(projects_dir: &Path, project_ref: &str) -> Result<PathBuf, Status> {
    let project = crate::project_storage::find_project_by_ref(projects_dir, project_ref)
        .map_err(|e| Status::internal(format!("read project registry: {e}")))?
        .ok_or_else(|| Status::not_found(format!("no project '{project_ref}'")))?;
    let repo_path = PathBuf::from(&project.main_repo_path);
    if !repo_path.exists() {
        return Err(Status::failed_precondition(format!(
            "project '{}' points at {}, which does not exist",
            project.name,
            repo_path.display()
        )));
    }
    Ok(repo_path)
}

/// The argv the child is spawned with, front-loaded with a `setpriv` privilege drop when the
/// target OS user differs from the daemon's own identity (reusing
/// [`crate::pty_runtime::wrap_argv_for_privilege_drop`], so the PTY and pipe paths cannot diverge).
pub fn git_argv(
    verb: GitVerb,
    repo_path: &Path,
    os_user: Option<&str>,
) -> Result<Vec<String>, Status> {
    match os_user {
        None => Ok(git_child_argv(verb, repo_path)),
        Some(os_user) => Ok(git_child_command(verb, repo_path, os_user)?.argv),
    }
}

/// The argv and environment a git child for `os_user` is spawned with, resolved together so the
/// passwd lookup happens once.
pub fn git_child_command(
    verb: GitVerb,
    repo_path: &Path,
    os_user: &str,
) -> Result<GitChildCommand, Status> {
    let target = crate::pty_runtime::resolve_pty_os_user(os_user)
        .map_err(|e| Status::internal(format!("cannot resolve os_user '{os_user}': {e}")))?;
    let (current_uid, current_gid) = daemon_identity();
    Ok(GitChildCommand {
        argv: git_argv_as_user(verb, repo_path, &target, current_uid, current_gid),
        env: git_child_env(&target),
    })
}

/// The command line and environment of one git child.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitChildCommand {
    pub argv: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// The argv for a git child that must run as the already-resolved `target` user, given the identity
/// the daemon itself runs under.
///
/// Split out from [`git_argv`] so the privilege drop can be exercised with ids the test host does
/// not have to own: the passwd lookup is the only part that needs a real account.
pub fn git_argv_as_user(
    verb: GitVerb,
    repo_path: &Path,
    target: &ResolvedPtyUser,
    current_uid: u32,
    current_gid: u32,
) -> Vec<String> {
    let argv = git_child_argv(verb, repo_path);
    if crate::pty_runtime::pty_requires_privilege_drop(
        target.uid,
        target.gid,
        current_uid,
        current_gid,
    ) {
        crate::pty_runtime::wrap_argv_for_privilege_drop(&argv, target.uid, target.gid)
    } else {
        argv
    }
}

/// The environment a git child runs with, and the only environment it gets: it is spawned with the
/// daemon's own environment cleared.
///
/// `setpriv` preserves the environment across the uid boundary, so inheriting would hand a process
/// running as somebody else every variable the daemon was started with — including
/// `LIVEKIT_API_SECRET`, which is the session-token signing key. `git receive-pack` runs the
/// repository's hooks and `git upload-pack` honours `uploadpack.packObjectsHook`, so that
/// environment is reachable by repository-controlled code. Inheriting `HOME` is wrong for a second
/// reason: git would read the daemon's `.gitconfig` instead of the project owner's.
fn git_child_env(target: &ResolvedPtyUser) -> Vec<(String, String)> {
    let home = PathBuf::from(&target.home_dir);
    let path_extra = crate::tddy_user_config::spawn_path_extra_for_home(&home);
    crate::pty_runtime::pty_user_env_overrides(&home, path_extra.as_deref())
}

/// `git <subcommand> -- <repo>`. The `--` matters because `main_repo_path` comes from a
/// hand-editable `projects.yaml`: a path beginning with `-` would otherwise be parsed as an option.
fn git_child_argv(verb: GitVerb, repo_path: &Path) -> Vec<String> {
    vec![
        "git".to_string(),
        verb.git_subcommand().to_string(),
        "--".to_string(),
        repo_path.to_string_lossy().into_owned(),
    ]
}

/// The uid/gid the daemon process itself runs under.
fn daemon_identity() -> (u32, u32) {
    unsafe { (libc::getuid(), libc::getgid()) }
}

/// Everything an authorized `Serve` open resolved to. Nothing is spawned until this exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedGitRequest {
    /// The GitHub login the `session_token` resolved to, for the admission log line.
    pub github_user: String,
    /// The OS user the git child runs as.
    pub os_user: String,
    /// The project's `main_repo_path`, read from the registry — never from the request.
    pub repo_path: PathBuf,
    pub verb: GitVerb,
}

/// A ceiling on how many git children run at once.
///
/// Every admitted `Serve` open is a real process working against a real object database. Without a
/// ceiling one authenticated client can open streams until the host's process table is exhausted,
/// which takes down every session on the host and not just this service.
pub struct GitStreamSlots {
    permits: Arc<Semaphore>,
    capacity: usize,
}

/// One taken slot, held for as long as the stream that took it. Dropping it readmits a client.
#[derive(Debug)]
pub struct GitStreamSlot {
    _permit: OwnedSemaphorePermit,
}

impl GitStreamSlots {
    pub fn new(capacity: usize) -> Self {
        Self {
            permits: Arc::new(Semaphore::new(capacity)),
            capacity,
        }
    }

    /// Take a slot, or refuse the stream. Refusing is deliberate: queueing instead would hold the
    /// client on an open stream with no way to tell how long it will wait.
    pub fn acquire(&self) -> Result<GitStreamSlot, Status> {
        self.permits
            .clone()
            .try_acquire_owned()
            .map(|permit| GitStreamSlot { _permit: permit })
            .map_err(|_| Status {
                code: Code::ResourceExhausted,
                message: format!(
                    "the daemon is already serving {} git streams; retry when one finishes",
                    self.capacity
                ),
            })
    }
}

/// Serves daemon projects as git remotes over any tddy-rpc transport.
pub struct RemoteGitServiceImpl {
    user_resolver: UserResolver,
    projects_dir_resolver: ProjectsDirResolver,
    config: Arc<DaemonConfig>,
    stream_slots: GitStreamSlots,
}

/// Everything `serve` resolved before spawning anything: the admission decision, the command it
/// produced, and the concurrency slot the stream holds for its lifetime.
struct AdmittedGitStream {
    request: AuthorizedGitRequest,
    command: GitChildCommand,
    slot: GitStreamSlot,
}

impl RemoteGitServiceImpl {
    pub fn new(
        user_resolver: UserResolver,
        projects_dir_resolver: ProjectsDirResolver,
        config: Arc<DaemonConfig>,
    ) -> Self {
        Self {
            user_resolver,
            projects_dir_resolver,
            config,
            stream_slots: GitStreamSlots::new(MAX_CONCURRENT_GIT_STREAMS),
        }
    }

    /// The complete admission decision for one `Serve` open, made **before** any process is
    /// spawned: token → GitHub user → OS user → project → repo path → verb.
    pub fn authorize_open(&self, open: &GitOpen) -> Result<AuthorizedGitRequest, Status> {
        let github_user = (self.user_resolver)(&open.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session token"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| {
                Status::permission_denied(format!("github user '{github_user}' has no os_user"))
            })?
            .to_string();
        let projects_dir = (self.projects_dir_resolver)(&os_user).ok_or_else(|| {
            Status::internal(format!(
                "cannot resolve the project registry of os_user '{os_user}'"
            ))
        })?;
        let repo_path = resolve_project_repo(&projects_dir, &open.project_ref)?;
        let verb = resolve_git_verb(&open.verb)?;
        Ok(AuthorizedGitRequest {
            github_user,
            os_user,
            repo_path,
            verb,
        })
    }

    /// Admission plus everything spawning needs. Nothing here starts a process: it either yields a
    /// resolved command and a concurrency slot, or a `Status` naming why the stream is refused.
    fn admit(&self, open: &GitOpen) -> Result<AdmittedGitStream, Status> {
        let request = self.authorize_open(open)?;
        let command = git_child_command(request.verb, &request.repo_path, &request.os_user)?;
        let slot = self.stream_slots.acquire()?;
        Ok(AdmittedGitStream {
            request,
            command,
            slot,
        })
    }
}

/// Log a refused open. The `session_token` is deliberately absent — it is a live credential, and it
/// is never written to the journal.
fn log_refused_open(project_ref: &str, verb: &str, status: &Status) {
    log::warn!(
        target: LOG_TARGET,
        "refusing a git stream for project '{project_ref}' verb '{verb}': {:?}: {}",
        status.code(),
        status.message()
    );
}

/// The `Serve` response stream: the relay's frames, adapted to the shape the generated service
/// trait requires.
pub struct GitServerFrameStream {
    rx: mpsc::Receiver<Result<GitServerFrame, Status>>,
}

impl futures_util::stream::Stream for GitServerFrameStream {
    type Item = Result<GitServerFrame, Status>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}

impl Unpin for GitServerFrameStream {}

#[async_trait::async_trait]
impl tddy_service::proto::remote_git::RemoteGitService for RemoteGitServiceImpl {
    type ServeStream = GitServerFrameStream;

    async fn serve(
        &self,
        request: tddy_rpc::Request<tddy_rpc::Streaming<GitClientFrame>>,
    ) -> Result<tddy_rpc::Response<Self::ServeStream>, Status> {
        use futures_util::StreamExt as _;

        let mut inbound = request.into_inner();
        let mut first = inbound
            .next()
            .await
            .ok_or_else(|| Status::invalid_argument("a Serve stream ended before its first frame"))?
            .map_err(|e| Status::internal(format!("read the first Serve frame: {e}")))?;

        // The first frame's payload fields are relayed like any other frame's, once the child
        // exists — a client that sends stdin alongside its `open` must not lose those bytes.
        let opening_stdin = std::mem::take(&mut first.stdin);
        let opening_stdin_eof = first.stdin_eof;

        let open = open_from_first_frame(first).inspect_err(|status| {
            log::warn!(
                target: LOG_TARGET,
                "refusing a git stream whose first frame carried no open: {:?}: {}",
                status.code(),
                status.message()
            );
        })?;
        let AdmittedGitStream {
            request,
            command,
            slot,
        } = self.admit(&open).inspect_err(|status| {
            log_refused_open(&open.project_ref, &open.verb, status);
        })?;

        let (relay, frames) =
            GitChildRelay::spawn_with_env(command.argv, request.repo_path.clone(), command.env)
                .inspect_err(|status| log_refused_open(&open.project_ref, &open.verb, status))?;
        log::info!(
            target: LOG_TARGET,
            "serving git {} to github user '{}' as os_user '{}' on project '{}' at {} (pid {})",
            request.verb.git_subcommand(),
            request.github_user,
            request.os_user,
            open.project_ref,
            request.repo_path.display(),
            relay.pid()
        );

        // The relay lives in this task and nowhere else, so the child's lifetime is exactly the
        // inbound stream's: when the client closes it — or the transport closes it because the peer
        // left — the loop ends, the relay drops, and the child's process group is signalled. The
        // concurrency slot is released at the same moment, and not before.
        tokio::spawn(async move {
            let pid = relay.pid();
            if let Err(e) =
                relay_client_frames(inbound, &relay, opening_stdin, opening_stdin_eof).await
            {
                log::warn!(target: LOG_TARGET, "git child {pid}: inbound stream ended early: {e}");
            }
            drop(relay);
            drop(slot);
        });

        Ok(tddy_rpc::Response::new(GitServerFrameStream { rx: frames }))
    }
}

/// Relay the client's frames into the child's stdin until the inbound stream ends.
///
/// Every way this can stop short is an `Err`, so the caller logs all of them in one place instead
/// of repeating the same warning at each `break`.
async fn relay_client_frames(
    mut inbound: tddy_rpc::Streaming<GitClientFrame>,
    relay: &GitChildRelay,
    opening_stdin: Vec<u8>,
    opening_stdin_eof: bool,
) -> Result<(), Status> {
    use futures_util::StreamExt as _;

    relay_stdin(relay, opening_stdin, opening_stdin_eof).await?;
    while let Some(frame) = inbound.next().await {
        let frame = frame.map_err(|e| Status::internal(format!("read a client frame: {e}")))?;
        relay_stdin(relay, frame.stdin, frame.stdin_eof).await?;
    }
    Ok(())
}

/// Apply one frame's stdin payload to the child.
async fn relay_stdin(relay: &GitChildRelay, stdin: Vec<u8>, stdin_eof: bool) -> Result<(), Status> {
    if !stdin.is_empty() {
        relay.send_stdin(stdin).await?;
    }
    if stdin_eof {
        relay.close_stdin().await?;
    }
    Ok(())
}

/// A spawned git child with its stdio relayed onto a frame stream.
///
/// Dropping the relay terminates the child (SIGTERM, then SIGKILL after a grace period) — that is
/// how a connection's end drops the git process it asked for.
pub struct GitChildRelay {
    pid: u32,
    /// The child's stdin, until the client closes it. Held behind a lock because a `Serve` stream
    /// writes from the task that reads client frames while the pumps run on their own tasks.
    stdin: Mutex<Option<tokio::process::ChildStdin>>,
    /// Dropped with the relay — that closed channel is what tells the supervisor task the
    /// connection is over and the child must go.
    _connection_alive: oneshot::Sender<()>,
}

/// The sink a relay's frames are written to. Bounded — see [`GIT_FRAME_CHANNEL_CAPACITY`].
type FrameSender = mpsc::Sender<Result<GitServerFrame, Status>>;

/// The stream of frames one relay produces.
pub type GitServerFrames = mpsc::Receiver<Result<GitServerFrame, Status>>;

impl GitChildRelay {
    /// Spawn `argv` in `cwd` with the daemon's own environment left in place.
    ///
    /// The name states the whole precondition, because the failure it guards against is silent: a
    /// child that crosses a uid boundary this way keeps the daemon's `HOME` (so git reads the wrong
    /// config) and inherits every secret the daemon was started with, `setpriv` being deliberately
    /// environment-preserving. Impersonation must go through [`GitChildRelay::spawn_with_env`],
    /// which clears the environment and hands the child one built for the target user by
    /// [`git_child_env`]. `serve` uses that path exclusively.
    pub fn spawn_under_daemon_identity(
        argv: Vec<String>,
        cwd: PathBuf,
    ) -> Result<(GitChildRelay, GitServerFrames), Status> {
        Self::spawn_child(argv, cwd, None)
    }

    /// Spawn `argv` in `cwd` with **exactly** `env` and nothing else: the daemon's own environment
    /// is cleared first, so no variable it was started with reaches the child.
    pub fn spawn_with_env(
        argv: Vec<String>,
        cwd: PathBuf,
        env: Vec<(String, String)>,
    ) -> Result<(GitChildRelay, GitServerFrames), Status> {
        Self::spawn_child(argv, cwd, Some(env))
    }

    /// Spawn the child with all three stdio streams piped, and start pumping:
    /// stdout → `GitServerFrame.stdout`, stderr → `GitServerFrame.stderr`, each chunked to at most
    /// [`MAX_GIT_FRAME_BYTES`]; on exit, one final frame carrying `exit_code` and `done = true`.
    fn spawn_child(
        argv: Vec<String>,
        cwd: PathBuf,
        env: Option<Vec<(String, String)>>,
    ) -> Result<(GitChildRelay, GitServerFrames), Status> {
        let (program, args) = argv
            .split_first()
            .ok_or_else(|| Status::internal("cannot spawn an empty argv"))?;
        let mut command = tokio::process::Command::new(program);
        command
            .args(args)
            // git resolves everything relative — hooks, alternates, the worktree — against this
            // directory, and it is the registry's `main_repo_path`, never the client's.
            .current_dir(&cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // Its own process group, so teardown can signal the workers git forks for itself
            // (`pack-objects`) and not just the process the daemon spawned.
            .process_group(0);
        if let Some(env) = env {
            command.env_clear().envs(env);
        }
        let mut child = command
            .spawn()
            .map_err(|e| Status::internal(format!("spawn {program}: {e}")))?;

        let pid = child
            .id()
            .ok_or_else(|| Status::internal("the child exited before it reported a pid"))?;
        let stdin = child.stdin.take();
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Status::internal("child stdout was not piped"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| Status::internal("child stderr was not piped"))?;

        let (frames, rx) = mpsc::channel(GIT_FRAME_CHANNEL_CAPACITY);
        let progress = Arc::new(PumpProgress::default());
        let pumps = vec![
            tokio::spawn(pump_output(
                stdout,
                frames.clone(),
                OutputStream::Stdout,
                progress.clone(),
            )),
            tokio::spawn(pump_output(
                stderr,
                frames.clone(),
                OutputStream::Stderr,
                progress.clone(),
            )),
        ];
        let (connection_alive, connection_ended) = oneshot::channel();
        tokio::spawn(supervise_child(
            child,
            pid,
            connection_ended,
            Pumps { pumps, progress },
            frames,
        ));

        Ok((
            GitChildRelay {
                pid,
                stdin: Mutex::new(stdin),
                _connection_alive: connection_alive,
            },
            rx,
        ))
    }

    /// Write bytes to the child's stdin, verbatim.
    pub async fn send_stdin(&self, data: Vec<u8>) -> Result<(), Status> {
        let mut stdin = self.stdin.lock().await;
        let pipe = stdin
            .as_mut()
            .ok_or_else(|| Status::failed_precondition("the child's stdin is already closed"))?;
        pipe.write_all(&data)
            .await
            .map_err(|e| Status::internal(format!("write to child stdin: {e}")))?;
        pipe.flush()
            .await
            .map_err(|e| Status::internal(format!("flush child stdin: {e}")))
    }

    /// Close the child's stdin. `git-upload-pack` completes its negotiation on EOF.
    pub async fn close_stdin(&self) -> Result<(), Status> {
        let pipe = self.stdin.lock().await.take();
        match pipe {
            Some(mut pipe) => pipe
                .shutdown()
                .await
                .map_err(|e| Status::internal(format!("close child stdin: {e}"))),
            None => Ok(()),
        }
    }

    /// The child's process id, for lifecycle assertions.
    pub fn pid(&self) -> u32 {
        self.pid
    }
}

/// Which `GitServerFrame` field a pump writes into. The two streams never share a field: git
/// writes progress on stderr while the pack goes down stdout, and interleaving them corrupts it.
#[derive(Clone, Copy)]
enum OutputStream {
    Stdout,
    Stderr,
}

impl OutputStream {
    fn frame(self, payload: Vec<u8>) -> GitServerFrame {
        match self {
            OutputStream::Stdout => GitServerFrame {
                stdout: payload,
                ..Default::default()
            },
            OutputStream::Stderr => GitServerFrame {
                stderr: payload,
                ..Default::default()
            },
        }
    }
}

/// What the output pumps are doing, shared with teardown.
///
/// A pump that has stopped can have stopped for two very different reasons: the pipe has no more
/// bytes to give yet (a worker git forked still holds it open), or the wire is slower than the
/// child and the pump is parked writing a frame. Only the first is a wedge worth killing the
/// process group over — mistaking the second for it would truncate a pack on a slow link.
#[derive(Default)]
struct PumpProgress {
    /// How many pumps are parked writing a frame right now.
    writing: AtomicUsize,
    /// Frames handed over so far, so a slow-but-moving wire is never mistaken for a stall.
    frames_sent: AtomicU64,
}

impl PumpProgress {
    fn snapshot(&self) -> u64 {
        self.frames_sent.load(Ordering::Relaxed)
    }

    fn is_moving(&self, since: u64) -> bool {
        self.snapshot() != since || self.writing.load(Ordering::Relaxed) > 0
    }
}

/// The output pumps of one child, with the progress they report.
struct Pumps {
    pumps: Vec<tokio::task::JoinHandle<()>>,
    progress: Arc<PumpProgress>,
}

/// Relay one of the child's output pipes to the frame stream until it reaches EOF. Reads are
/// bounded by [`MAX_GIT_FRAME_BYTES`], so a frame is never large enough to be chunk-framed, and
/// each frame is *awaited* onto the bounded channel, so a wire slower than the child stops the pump
/// rather than accumulating the pack in the daemon's heap.
async fn pump_output<R>(
    mut reader: R,
    frames: FrameSender,
    stream: OutputStream,
    progress: Arc<PumpProgress>,
) where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut buf = vec![0u8; MAX_GIT_FRAME_BYTES];
    loop {
        match reader.read(&mut buf).await {
            Ok(0) => return,
            Ok(n) => {
                let frame = stream.frame(buf[..n].to_vec());
                if send_frame(&frames, Ok(frame), &progress).await.is_err() {
                    return;
                }
            }
            Err(e) => {
                let failure = Err(Status::internal(format!("read child output: {e}")));
                let _ = send_frame(&frames, failure, &progress).await;
                return;
            }
        }
    }
}

/// Hand one frame to the bounded channel, recording that the pump is parked on the wire while it
/// waits. `Err` means the consumer is gone.
async fn send_frame(
    frames: &FrameSender,
    frame: Result<GitServerFrame, Status>,
    progress: &PumpProgress,
) -> Result<(), ()> {
    progress.writing.fetch_add(1, Ordering::Relaxed);
    let sent = frames.send(frame).await;
    progress.writing.fetch_sub(1, Ordering::Relaxed);
    progress.frames_sent.fetch_add(1, Ordering::Relaxed);
    sent.map_err(|_| ())
}

/// Wait for the child, or terminate it when the connection ends, then close the stream with the
/// exit status.
///
/// The done frame is emitted only after both pumps have reached EOF, so output the child wrote
/// just before exiting cannot be lost to the teardown.
async fn supervise_child(
    mut child: tokio::process::Child,
    pid: u32,
    connection_ended: oneshot::Receiver<()>,
    pumps: Pumps,
    frames: FrameSender,
) {
    let status = tokio::select! {
        status = child.wait() => status,
        _ = connection_ended => terminate_child(&mut child, pid).await,
    };
    drain_pumps(pumps, pid).await;
    match status {
        Ok(status) => {
            let exit_code = exit_code_of(&status);
            log::info!(target: LOG_TARGET, "git child {pid} exited with code {exit_code}");
            let _ = frames
                .send(Ok(GitServerFrame {
                    exit_code,
                    done: true,
                    ..Default::default()
                }))
                .await;
        }
        Err(e) => {
            log::warn!(target: LOG_TARGET, "git child {pid} could not be reaped: {e}");
            let _ = frames
                .send(Err(Status::internal(format!("wait for git child: {e}"))))
                .await;
        }
    }
}

/// Wait for both output pumps to finish, so nothing the child wrote just before exiting is lost to
/// the teardown — but never wait forever.
///
/// A process the child forked can inherit its stdout and outlive it (`git-upload-pack` forks
/// `pack-objects`), and then the pipe never reaches EOF. Waiting on it indefinitely would leave the
/// client on a `done` frame that never arrives with no error to explain it, which is precisely the
/// silent wedge this service is built to avoid. Once the pumps have gone [`PUMP_STALL_TIMEOUT`]
/// without delivering a frame or being parked writing one, the process group is signalled so the
/// pipe closes and the stream can be terminated.
async fn drain_pumps(pumps: Pumps, pid: u32) {
    let Pumps { pumps, progress } = pumps;
    let aborts: Vec<_> = pumps.iter().map(|pump| pump.abort_handle()).collect();
    let mut joined = futures_util::future::join_all(pumps);
    loop {
        let before = progress.snapshot();
        if tokio::time::timeout(PUMP_STALL_TIMEOUT, &mut joined)
            .await
            .is_ok()
        {
            return;
        }
        if !progress.is_moving(before) {
            break;
        }
    }
    log::warn!(
        target: LOG_TARGET,
        "git child {pid} is gone but its output pipe is still held — a process it forked inherited \
         it; killing the process group so the stream can be closed"
    );
    signal_process_group(pid, libc::SIGKILL);
    if tokio::time::timeout(CHILD_TERMINATION_GRACE, &mut joined)
        .await
        .is_err()
    {
        // The done frame has to be the last thing on this stream, so no pump may outlive this
        // point and write after it.
        for abort in aborts {
            abort.abort();
        }
    }
}

/// Signal the child's whole process group — SIGTERM, then SIGKILL once the grace period is up —
/// and reap it. A grandchild left behind would hold the repository's object database open.
async fn terminate_child(
    child: &mut tokio::process::Child,
    pid: u32,
) -> std::io::Result<ExitStatus> {
    signal_process_group(pid, libc::SIGTERM);
    if let Ok(status) = tokio::time::timeout(CHILD_TERMINATION_GRACE, child.wait()).await {
        return status;
    }
    log::warn!(
        target: "tddy_daemon::remote_git_service",
        "git child {pid} ignored SIGTERM for {}s; killing its process group",
        CHILD_TERMINATION_GRACE.as_secs()
    );
    signal_process_group(pid, libc::SIGKILL);
    child.wait().await
}

fn signal_process_group(pid: u32, signal: libc::c_int) {
    unsafe {
        libc::kill(-(pid as i32), signal);
    }
}

/// The status git should see. A child killed by a signal has no exit code of its own, so it is
/// reported the way a shell reports one.
fn exit_code_of(status: &ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt;
    status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(0))
}
