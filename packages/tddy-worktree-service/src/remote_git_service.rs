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

mod child_relay;
mod pumps;
mod request;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, oneshot, Mutex, OwnedSemaphorePermit, Semaphore};

use tddy_rpc::{Code, Status};
use tddy_service::proto::remote_git::{GitClientFrame, GitOpen, GitServerFrame};

use crate::remote_git_service::request::GitChildCommand;
use tddy_daemon_kernel::config::DaemonConfig;

/// The log target every line in this module carries, so an operator can follow one clone through
/// the journal.
const LOG_TARGET: &str = "tddy_daemon::remote_git_service";

/// Resolves a `session_token` to a GitHub login. The daemon's own resolver
/// ([`crate::auth`]) verifies the signature under the key the token names, the expiry, and that
/// the token is access-kind.
pub type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// Resolves an OS user to that user's project registry directory
/// (`~/.tddy/projects/`). Mirrors `DaemonSessionHost`'s sessions-base resolver.
pub type ProjectsDirResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;

/// Resolves an OS user to that user's sessions base (`~/.tddy/`). Used to discover SSH-backed
/// sessions when choosing where pack verbs run.
pub type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;

/// Everything [`PackExecutionResolver`] needs to decide local vs remote pack spawn for one open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackExecutionContext {
    pub os_user: String,
    pub project_ref: String,
    pub local_repo_path: PathBuf,
}

/// Maps an admitted open to [`PackExecution`]. Production wiring lists the OS user's sessions and
/// picks an SSH-backed row for the project; tests inject a stub.
pub type PackExecutionResolver = Arc<dyn Fn(PackExecutionContext) -> PackExecution + Send + Sync>;

/// Today's behaviour: always pack the registry's local `main_repo_path`.
pub fn local_only_pack_execution_resolver() -> PackExecutionResolver {
    Arc::new(|ctx| pack_execution_for_session(ctx.local_repo_path, "", ""))
}

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

pub use crate::remote_git_service::request::{
    git_argv, git_argv_as_user, git_child_command, open_from_first_frame,
    pack_execution_for_session, resolve_git_verb, resolve_project_repo, GitVerb, PackExecution,
};

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
    pack_execution_resolver: PackExecutionResolver,
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
        Self::new_with_pack_execution_resolver(
            user_resolver,
            projects_dir_resolver,
            config,
            local_only_pack_execution_resolver(),
        )
    }

    pub fn new_with_pack_execution_resolver(
        user_resolver: UserResolver,
        projects_dir_resolver: ProjectsDirResolver,
        config: Arc<DaemonConfig>,
        pack_execution_resolver: PackExecutionResolver,
    ) -> Self {
        Self {
            user_resolver,
            projects_dir_resolver,
            config,
            pack_execution_resolver,
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

        let execution = (self.pack_execution_resolver)(PackExecutionContext {
            os_user: request.os_user.clone(),
            project_ref: open.project_ref.clone(),
            local_repo_path: request.repo_path.clone(),
        });
        let (relay, frames) = GitChildRelay::spawn_pack_verb(&execution, command.argv, command.env)
            .await
            .inspect_err(|status| log_refused_open(&open.project_ref, &open.verb, status))?;
        let spawn_target = match &execution {
            PackExecution::Local { repo_path } => repo_path.display().to_string(),
            PackExecution::Remote {
                ssh_config_host,
                remote_repo_path,
            } => format!("ssh:{ssh_config_host}:{remote_repo_path}"),
        };
        log::info!(
            target: LOG_TARGET,
            "serving git {} to github user '{}' as os_user '{}' on project '{}' at {} (pid {})",
            request.verb.git_subcommand(),
            request.github_user,
            request.os_user,
            open.project_ref,
            spawn_target,
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
    stdin: Mutex<Option<tddy_remote_shell::ShellStdin>>,
    /// Dropped with the relay — that closed channel is what tells the supervisor task the
    /// connection is over and the child must go.
    _connection_alive: oneshot::Sender<()>,
}

/// The sink a relay's frames are written to. Bounded — see [`GIT_FRAME_CHANNEL_CAPACITY`].
type FrameSender = mpsc::Sender<Result<GitServerFrame, Status>>;

pub use crate::remote_git_service::child_relay::GitServerFrames;
