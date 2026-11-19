//! A remote shell that lives in this process: a test double for everything that takes the port.
//!
//! Scripted, not simulated. A command is answered by what a test said it answers
//! ([`InMemoryShell::answers`], [`InMemoryShell::answers_when`]); a command nothing answers exits
//! 127, as a shell's unknown command does. Files written through the port are kept in a map and
//! read back from it. Every call is recorded ([`InMemoryShell::calls`]).

use std::collections::{BTreeMap, HashMap};
use std::ffi::OsString;
use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _, DuplexStream};
use tokio::sync::oneshot;

use crate::{
    ExecOutput, ExecRequest, FileMode, InteractiveCommand, InteractiveExit, RemoteShell,
    RemoteShellError, ShellAddress, StreamedExec, StreamedExit,
};

/// How much a scripted stream's pipe holds before its writer waits for the reader.
const SCRIPTED_PIPE_BYTES: usize = 64 * 1024;

/// One call made on an [`InMemoryShell`], in the order it was made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellCall {
    Exec(ExecRequest),
    ExecStreamed(String),
    Interactive(String),
    ReadFile(String),
    WriteFile {
        path: String,
        bytes: Vec<u8>,
        mode: FileMode,
    },
}

type Answerer = Arc<dyn Fn(&ExecRequest) -> Option<ExecOutput> + Send + Sync>;

enum Scripted {
    Output(ExecOutput),
    Hangs,
}

#[derive(Default)]
struct State {
    scripted: HashMap<String, Scripted>,
    answerers: Vec<Answerer>,
    unreachable: Option<String>,
    files: BTreeMap<String, (Vec<u8>, FileMode)>,
    streams: HashMap<String, StreamedExec>,
    calls: Vec<ShellCall>,
}

/// The in-memory shell. Clones share one state, so a test keeps a handle to the shell it gave
/// away.
#[derive(Clone)]
pub struct InMemoryShell {
    address: ShellAddress,
    state: Arc<Mutex<State>>,
}

impl InMemoryShell {
    /// The tag of this transport in a [`ShellAddress`].
    pub const TRANSPORT: &'static str = "in-memory";

    /// A reachable shell named `name` that answers nothing yet and holds no file.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            address: ShellAddress::new(Self::TRANSPORT, name),
            state: Arc::default(),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// From now on `command`, exactly as written, is answered with `output`.
    pub fn answers(&self, command: impl Into<String>, output: ExecOutput) -> &Self {
        self.state()
            .scripted
            .insert(command.into(), Scripted::Output(output));
        self
    }

    /// From now on a command no exact answer is scripted for is offered to `answerer`; the first
    /// answerer to return an output answers it.
    pub fn answers_when(
        &self,
        answerer: impl Fn(&ExecRequest) -> Option<ExecOutput> + Send + Sync + 'static,
    ) -> &Self {
        self.state().answerers.push(Arc::new(answerer));
        self
    }

    /// From now on `command` never finishes: it fails [`RemoteShellError::TimedOut`] after its
    /// request's deadline, without waiting it out.
    pub fn hangs_on(&self, command: impl Into<String>) -> &Self {
        self.state()
            .scripted
            .insert(command.into(), Scripted::Hangs);
        self
    }

    /// From now on every call fails [`RemoteShellError::Unreachable`] with `detail`.
    pub fn is_unreachable(&self, detail: impl Into<String>) -> &Self {
        self.state().unreachable = Some(detail.into());
        self
    }

    /// The target holds `bytes` at `path`, with `mode`.
    pub fn has_file(
        &self,
        path: impl Into<String>,
        bytes: impl Into<Vec<u8>>,
        mode: FileMode,
    ) -> &Self {
        self.state().files.insert(path.into(), (bytes.into(), mode));
        self
    }

    /// The next [`RemoteShell::exec_streamed`] of `command` is the stream the returned script
    /// drives: what it prints, what it was sent, and how it ends.
    pub fn streams(&self, command: impl Into<String>) -> ScriptedStream {
        let (stdin_writer, stdin_reader) = tokio::io::duplex(SCRIPTED_PIPE_BYTES);
        let (stdout_writer, stdout_reader) = tokio::io::duplex(SCRIPTED_PIPE_BYTES);
        let (stderr_writer, stderr_reader) = tokio::io::duplex(SCRIPTED_PIPE_BYTES);
        let (exit, exited) = oneshot::channel();
        let streamed = StreamedExec {
            stdin: Box::new(stdin_writer),
            stdout: Box::new(stdout_reader),
            stderr: Box::new(stderr_reader),
            exit: StreamedExit::new(async move {
                exited.await.map_err(|_| {
                    RemoteShellError::Spawn(
                        "the scripted stream was dropped before it ended".to_string(),
                    )
                })
            }),
        };
        self.state().streams.insert(command.into(), streamed);
        ScriptedStream {
            stdin: tokio::sync::Mutex::new(stdin_reader),
            stdout: tokio::sync::Mutex::new(Some(stdout_writer)),
            stderr: tokio::sync::Mutex::new(Some(stderr_writer)),
            exit: Mutex::new(Some(exit)),
        }
    }

    /// Every call made so far, oldest first.
    pub fn calls(&self) -> Vec<ShellCall> {
        self.state().calls.clone()
    }

    /// The command line of every [`RemoteShell::exec`] so far, oldest first.
    pub fn commands_run(&self) -> Vec<String> {
        self.state()
            .calls
            .iter()
            .filter_map(|call| match call {
                ShellCall::Exec(request) => Some(request.command.clone()),
                _ => None,
            })
            .collect()
    }

    /// The bytes at `path`, when the target holds a file there.
    pub fn file(&self, path: &str) -> Option<Vec<u8>> {
        self.state().files.get(path).map(|(bytes, _)| bytes.clone())
    }

    /// The mode of the file at `path`, when the target holds one there.
    pub fn file_mode(&self, path: &str) -> Option<FileMode> {
        self.state().files.get(path).map(|(_, mode)| *mode)
    }

    /// Records `call`, and refuses it when the target is unreachable.
    fn reached(&self, call: ShellCall) -> Result<MutexGuard<'_, State>, RemoteShellError> {
        let mut state = self.state();
        state.calls.push(call);
        match state.unreachable.clone() {
            Some(detail) => Err(RemoteShellError::Unreachable {
                address: self.address.clone(),
                detail,
            }),
            None => Ok(state),
        }
    }
}

#[async_trait]
impl RemoteShell for InMemoryShell {
    fn address(&self) -> ShellAddress {
        self.address.clone()
    }

    async fn exec(&self, request: ExecRequest) -> Result<ExecOutput, RemoteShellError> {
        let answerers = {
            let state = self.reached(ShellCall::Exec(request.clone()))?;
            let answerers = match state.scripted.get(&request.command) {
                Some(Scripted::Output(output)) => return Ok(output.clone()),
                Some(Scripted::Hangs) => {
                    return Err(RemoteShellError::TimedOut {
                        address: self.address.clone(),
                        after: request.deadline,
                        command: crate::redact::command_head(&request.command),
                    });
                }
                None => state.answerers.clone(),
            };
            answerers
        };
        // Outside the lock: an answerer may look at this shell's files or calls.
        Ok(answerers
            .iter()
            .find_map(|answerer| answerer(&request))
            .unwrap_or_else(|| {
                ExecOutput::exited(
                    127,
                    format!("in-memory shell: nothing answers: {}\n", request.command),
                )
            }))
    }

    async fn exec_streamed(&self, command: &str) -> Result<StreamedExec, RemoteShellError> {
        self.reached(ShellCall::ExecStreamed(command.to_string()))?
            .streams
            .remove(command)
            .ok_or_else(|| {
                RemoteShellError::Spawn(format!(
                    "in-memory shell: no stream is scripted for: {command}"
                ))
            })
    }

    fn interactive(&self, command: &str) -> InteractiveCommand {
        self.state()
            .calls
            .push(ShellCall::Interactive(command.to_string()));
        InteractiveCommand {
            argv: ["in-memory-shell", self.address.name.as_str(), command]
                .into_iter()
                .map(OsString::from)
                .collect(),
            env: Vec::new(),
            exit: InteractiveExit::ClientStatus,
        }
    }

    async fn read_file(&self, path: &str) -> Result<Vec<u8>, RemoteShellError> {
        self.reached(ShellCall::ReadFile(path.to_string()))?
            .files
            .get(path)
            .map(|(bytes, _)| bytes.clone())
            .ok_or_else(|| RemoteShellError::Exited {
                status: 1,
                stderr: format!("cat: {path}: No such file or directory\n"),
            })
    }

    async fn write_file(
        &self,
        path: &str,
        bytes: &[u8],
        mode: FileMode,
    ) -> Result<(), RemoteShellError> {
        self.reached(ShellCall::WriteFile {
            path: path.to_string(),
            bytes: bytes.to_vec(),
            mode,
        })?
        .files
        .insert(path.to_string(), (bytes.to_vec(), mode));
        Ok(())
    }
}

/// The far end of a stream scripted with [`InMemoryShell::streams`]: the command's side of its
/// three pipes, and its exit.
pub struct ScriptedStream {
    stdin: tokio::sync::Mutex<DuplexStream>,
    stdout: tokio::sync::Mutex<Option<DuplexStream>>,
    stderr: tokio::sync::Mutex<Option<DuplexStream>>,
    exit: Mutex<Option<oneshot::Sender<i32>>>,
}

impl ScriptedStream {
    /// The next `count` bytes the caller wrote to the command's stdin.
    pub async fn stdin_exact(&self, count: usize) -> Vec<u8> {
        let mut received = vec![0; count];
        self.stdin
            .lock()
            .await
            .read_exact(&mut received)
            .await
            .expect("the caller closed the command's stdin before it wrote that much");
        received
    }

    /// Everything the caller writes to the command's stdin from now until it closes it.
    pub async fn stdin_to_end(&self) -> Vec<u8> {
        let mut received = Vec::new();
        self.stdin
            .lock()
            .await
            .read_to_end(&mut received)
            .await
            .expect("an in-memory pipe does not fail");
        received
    }

    /// The command prints `bytes` on its stdout.
    pub async fn prints(&self, bytes: impl AsRef<[u8]>) {
        Self::write(&self.stdout, bytes.as_ref()).await;
    }

    /// The command writes `bytes` to its stderr.
    pub async fn complains(&self, bytes: impl AsRef<[u8]>) {
        Self::write(&self.stderr, bytes.as_ref()).await;
    }

    async fn write(pipe: &tokio::sync::Mutex<Option<DuplexStream>>, bytes: &[u8]) {
        pipe.lock()
            .await
            .as_mut()
            .expect("the scripted stream has already ended")
            .write_all(bytes)
            .await
            .expect("the caller dropped the stream it was reading");
    }

    /// The command exits `status`: its stdout and stderr reach their end, and the caller's
    /// [`StreamedExit::wait`] returns.
    pub async fn ends_with(&self, status: i32) {
        self.stdout.lock().await.take();
        self.stderr.lock().await.take();
        let exit = self
            .exit
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        if let Some(exit) = exit {
            // A caller that dropped the stream is not waiting for its exit.
            let _ = exit.send(status);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::time::Duration;

    fn buildbox() -> InMemoryShell {
        InMemoryShell::new("buildbox")
    }

    #[tokio::test]
    async fn a_scripted_command_is_answered_with_its_output() {
        // Given
        let shell = buildbox();
        shell.answers("git rev-parse HEAD", ExecOutput::succeeded("abc123\n"));

        // When
        let output = shell.exec(ExecRequest::new("git rev-parse HEAD")).await;

        // Then
        assert_eq!(output, Ok(ExecOutput::succeeded("abc123\n")));
    }

    #[tokio::test]
    async fn a_command_nothing_answers_exits_127_naming_itself() {
        // Given
        let shell = buildbox();

        // When
        let output = shell.exec(ExecRequest::new("frobnicate")).await;

        // Then
        assert_eq!(
            output,
            Ok(ExecOutput::exited(
                127,
                "in-memory shell: nothing answers: frobnicate\n"
            ))
        );
    }

    #[tokio::test]
    async fn an_answerer_answers_what_no_exact_script_does_and_sees_the_stdin() {
        // Given
        let shell = buildbox();
        shell.answers_when(|request| {
            request
                .command
                .starts_with("cat")
                .then(|| ExecOutput::succeeded(request.stdin.clone().unwrap_or_default()))
        });

        // When
        let output = shell
            .exec(ExecRequest::new("cat -").with_stdin("fed"))
            .await;

        // Then
        assert_eq!(output, Ok(ExecOutput::succeeded("fed")));
    }

    #[tokio::test]
    async fn a_command_that_hangs_times_out_after_its_deadline_without_waiting() {
        // Given
        let shell = buildbox();
        shell.hangs_on("sleep 3600");

        // When
        let outcome = shell
            .exec(ExecRequest::new("sleep 3600").within(Duration::from_secs(900)))
            .await;

        // Then
        assert!(
            matches!(
                &outcome,
                Err(RemoteShellError::TimedOut { address, after, .. })
                    if *address == ShellAddress::new("in-memory", "buildbox")
                        && *after == Duration::from_secs(900)
            ),
            "{outcome:?}"
        );
    }

    #[tokio::test]
    async fn an_unreachable_shell_refuses_every_call_with_the_reason() {
        // Given
        let shell = buildbox();
        shell
            .answers("true", ExecOutput::succeeded(""))
            .has_file("/etc/motd", "hello", FileMode::TargetDefault)
            .is_unreachable("Connection refused");
        let unreachable = RemoteShellError::Unreachable {
            address: ShellAddress::new("in-memory", "buildbox"),
            detail: "Connection refused".to_string(),
        };

        // When
        let exec = shell.exec(ExecRequest::new("true")).await;
        let read = shell.read_file("/etc/motd").await;
        let write = shell
            .write_file("/etc/motd", b"bye", FileMode::TargetDefault)
            .await;
        let streamed = shell.exec_streamed("cat").await.map(|_| ());

        // Then
        assert_eq!(exec, Err(unreachable.clone()));
        assert_eq!(read, Err(unreachable.clone()));
        assert_eq!(write, Err(unreachable.clone()));
        assert_eq!(streamed, Err(unreachable));
        assert_eq!(shell.file("/etc/motd"), Some(b"hello".to_vec()));
    }

    #[tokio::test]
    async fn a_written_file_is_held_with_its_mode_and_read_back() {
        // Given
        let shell = buildbox();

        // When
        shell
            .write_file("/w/.tddy/session.json", b"{}", FileMode::OwnerOnly)
            .await
            .expect("an in-memory write succeeds");
        let read = shell.read_file("/w/.tddy/session.json").await;

        // Then
        assert_eq!(read, Ok(b"{}".to_vec()));
        assert_eq!(shell.file("/w/.tddy/session.json"), Some(b"{}".to_vec()));
        assert_eq!(
            shell.file_mode("/w/.tddy/session.json"),
            Some(FileMode::OwnerOnly)
        );
    }

    #[tokio::test]
    async fn a_missing_file_reads_as_a_failed_cat() {
        // Given
        let shell = buildbox();

        // When
        let read = shell.read_file("/nowhere").await;

        // Then
        assert_eq!(
            read,
            Err(RemoteShellError::Exited {
                status: 1,
                stderr: "cat: /nowhere: No such file or directory\n".to_string(),
            })
        );
    }

    #[tokio::test]
    async fn every_call_is_recorded_in_order() {
        // Given
        let shell = buildbox();
        let _script = shell.streams("git upload-pack -- '/r'");

        // When
        let _ = shell.exec(ExecRequest::new("true").with_stdin("in")).await;
        let _ = shell.write_file("/f", b"x", FileMode::TargetDefault).await;
        let _ = shell.read_file("/f").await;
        let _ = shell.exec_streamed("git upload-pack -- '/r'").await;
        let _ = shell.interactive("claude");

        // Then
        assert_eq!(
            shell.calls(),
            vec![
                ShellCall::Exec(ExecRequest::new("true").with_stdin("in")),
                ShellCall::WriteFile {
                    path: "/f".to_string(),
                    bytes: b"x".to_vec(),
                    mode: FileMode::TargetDefault,
                },
                ShellCall::ReadFile("/f".to_string()),
                ShellCall::ExecStreamed("git upload-pack -- '/r'".to_string()),
                ShellCall::Interactive("claude".to_string()),
            ]
        );
        assert_eq!(shell.commands_run(), vec!["true".to_string()]);
    }

    #[tokio::test]
    async fn a_scripted_stream_carries_both_directions_and_its_exit() {
        // Given
        let shell = buildbox();
        let script = shell.streams("git receive-pack -- '/r'");

        // When
        let mut streamed = shell
            .exec_streamed("git receive-pack -- '/r'")
            .await
            .expect("the scripted stream starts");
        streamed
            .stdin
            .write_all(b"0000")
            .await
            .expect("stdin is open");
        let sent = script.stdin_exact(4).await;
        script.prints("PACK").await;
        script.complains("unpacking").await;
        script.ends_with(7).await;
        let mut stdout = Vec::new();
        streamed
            .stdout
            .read_to_end(&mut stdout)
            .await
            .expect("stdout ends");
        let mut stderr = Vec::new();
        streamed
            .stderr
            .read_to_end(&mut stderr)
            .await
            .expect("stderr ends");
        let status = streamed.exit.wait().await;

        // Then
        assert_eq!(sent, b"0000");
        assert_eq!(stdout, b"PACK");
        assert_eq!(stderr, b"unpacking");
        assert_eq!(status, Ok(7));
    }

    #[tokio::test]
    async fn a_stream_nothing_scripted_cannot_be_started() {
        // Given
        let shell = buildbox();

        // When
        let refused = shell.exec_streamed("cat").await.map(|_| ());

        // Then
        assert_eq!(
            refused,
            Err(RemoteShellError::Spawn(
                "in-memory shell: no stream is scripted for: cat".to_string()
            ))
        );
    }

    #[test]
    fn the_interactive_command_names_the_target_and_the_command() {
        // Given
        let shell = buildbox();

        // When
        let interactive = shell.interactive("claude --resume");

        // Then
        assert_eq!(
            interactive,
            InteractiveCommand {
                argv: vec![
                    "in-memory-shell".into(),
                    "buildbox".into(),
                    "claude --resume".into()
                ],
                env: Vec::new(),
                exit: InteractiveExit::ClientStatus,
            }
        );
    }
}
