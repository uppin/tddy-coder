use crate::remote_git_service::pumps::pump_output;
use crate::remote_git_service::pumps::supervise_child;
use crate::remote_git_service::pumps::supervise_streamed;
use crate::remote_git_service::pumps::OutputStream;
use crate::remote_git_service::pumps::PumpProgress;
use crate::remote_git_service::pumps::Pumps;
use crate::remote_git_service::request::remote_pack_shell_command;
use crate::remote_git_service::request::PackExecution;
use crate::remote_git_service::GitChildRelay;
use crate::remote_git_service::GIT_FRAME_CHANNEL_CAPACITY;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tddy_rpc::Status;
use tddy_service::proto::remote_git::GitServerFrame;
use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, oneshot, Mutex};

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

    /// Spawn a pack verb through [`PackExecution`]. Local is [`spawn_with_env`]. Remote streams
    /// it through the remote-shell port's ssh transport to the session's Host alias.
    pub async fn spawn_pack_verb(
        execution: &PackExecution,
        argv: Vec<String>,
        env: Vec<(String, String)>,
    ) -> Result<(GitChildRelay, GitServerFrames), Status> {
        match execution {
            PackExecution::Local { repo_path } => {
                Self::spawn_with_env(argv, repo_path.clone(), env)
            }
            PackExecution::Remote {
                ssh_config_host,
                remote_repo_path,
            } => {
                // The port's ssh transport runs as the daemon and keeps agent, config, and PATH —
                // not the constructed env passed for a local `setpriv` git child.
                Self::spawn_remote_pack_verb(
                    &tddy_remote_shell::SshShell::new(ssh_config_host),
                    argv,
                    remote_repo_path,
                )
                .await
            }
        }
    }

    /// Start the pack verb `argv` names on the target `shell` reaches, packing `remote_repo_path`
    /// there, and relay it as [`Self::spawn_pack_verb`] relays a local child: stdin written
    /// through, stdout and stderr in frames of their own, one final `done` frame with the exit
    /// status. The repo path in `argv` is replaced with `remote_repo_path`, so the target never
    /// packs a path the session did not authorize.
    pub async fn spawn_remote_pack_verb(
        shell: &dyn tddy_remote_shell::RemoteShell,
        argv: Vec<String>,
        remote_repo_path: &str,
    ) -> Result<(GitChildRelay, GitServerFrames), Status> {
        let command = remote_pack_shell_command(&argv, remote_repo_path)
            .map_err(|e| Status::internal(format!("remote pack command: {e}")))?;
        let streamed = shell
            .exec_streamed(&command)
            .await
            .map_err(|error| Status::internal(error.to_string()))?;
        Ok(Self::spawn_streamed(streamed))
    }

    /// Relay a streamed remote command as a local child is relayed: its pipes become frames, and
    /// its exit becomes the one final `done` frame. It has no local pid (`0`).
    fn spawn_streamed(
        streamed: tddy_remote_shell::StreamedExec,
    ) -> (GitChildRelay, GitServerFrames) {
        let tddy_remote_shell::StreamedExec {
            stdin,
            stdout,
            stderr,
            exit,
        } = streamed;
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
        tokio::spawn(supervise_streamed(
            exit,
            connection_ended,
            Pumps { pumps, progress },
            frames,
        ));
        (
            GitChildRelay {
                pid: 0,
                stdin: Mutex::new(Some(stdin)),
                _connection_alive: connection_alive,
            },
            rx,
        )
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
                stdin: Mutex::new(
                    stdin.map(|stdin| Box::new(stdin) as tddy_remote_shell::ShellStdin),
                ),
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
