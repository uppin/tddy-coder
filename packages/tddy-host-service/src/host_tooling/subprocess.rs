use crate::host_tooling::PROBE_TIMEOUT;

/// What one probe command produced: how it exited, and everything it wrote.
#[cfg(unix)]
pub(crate) struct ProbeOutput {
    pub(crate) status: std::process::ExitStatus,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
}

/// The bytes a probe command wrote, once both of its pipes reached EOF.
#[cfg(unix)]
struct Written {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

/// One probe command, started as `os_user`, with its output being read on a thread of its own.
#[cfg(unix)]
pub(crate) struct StartedProbe {
    /// The command line, for a failure reason an operator can act on.
    command: String,
    /// `Err` when the command never started: there is nothing to wait for, and nothing to kill.
    pub(super) running: Result<RunningProbe, String>,
}

/// A probe command that is running right now, and the two handles onto it.
// TODO(host-desktops sweep): the engine widened StartedProbe::running to pub(super) but not the RunningProbe it names (E0446): drop this widening once the engine widens reached types
#[cfg(unix)]
pub(super) struct RunningProbe {
    /// The live child, held by the **parent** rather than by the reader thread. The parent is the
    /// side that owns the deadline, so it has to be the side that can end the child — and it ends
    /// it through the `Child` itself, never through a pid saved off to one side, because a pid
    /// outlives the process it named and the next signal sent to it lands on whatever the kernel
    /// has since reused it for.
    pub(super) child: std::process::Child,
    /// Everything the child wrote. One message, sent once both pipes are at EOF.
    written: std::sync::mpsc::Receiver<Written>,
}

/// Start one probe command as `os_user` without waiting for it.
///
/// `program` is an **absolute** path — see `probe`: `start_output_as_user` would resolve a relative
/// one against the daemon's own cwd rather than searching `PATH`.
///
/// Starting is not waiting: this returns as soon as the child exists, so `probe` can start all
/// three commands before collecting any of them and spend one deadline on the lot.
#[cfg(unix)]
pub(crate) fn start_probe(os_user: &str, program: &std::path::Path, args: &[&str]) -> StartedProbe {
    let owned_args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let command = format!("{} {}", program.display(), args.join(" "));
    StartedProbe {
        running: start_running(os_user, program, &owned_args)
            .map_err(|e| format!("could not run {command}: {e}")),
        command,
    }
}

/// Spawn the child and put a reader thread on its pipes.
#[cfg(unix)]
fn start_running(
    os_user: &str,
    program: &std::path::Path,
    args: &[String],
) -> anyhow::Result<RunningProbe> {
    let spawned = tddy_daemon_kernel::spawn_as_user::start_output_as_user(os_user, program, args)?;
    Ok(RunningProbe {
        written: read_both_pipes(spawned.stdout, spawned.stderr),
        child: spawned.child,
    })
}

/// Read a child's two pipes to EOF on a thread, and report the bytes on a channel.
///
/// The thread owns **only** the pipes, never the `Child`. That is what makes the parent's kill safe
/// and complete: killing the child closes both pipes, both reads hit EOF, and this thread ends by
/// itself. A thread that instead waited on the child — which is what `Command::output` does inside
/// `run_output_as_user` — is the thread that used to be left parked forever on a timeout.
///
/// Both pipes are read concurrently, for the reason `Command::output` reads them concurrently: a
/// child that fills one pipe's buffer while this thread blocks on the other never drains, and would
/// hang until the deadline killed it — turning an answer that was ready into a timeout.
#[cfg(unix)]
fn read_both_pipes(
    stdout: std::process::ChildStdout,
    stderr: std::process::ChildStderr,
) -> std::sync::mpsc::Receiver<Written> {
    let (tx, written) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let draining_stderr = std::thread::spawn(move || read_to_end(stderr));
        let stdout = read_to_end(stdout);
        // `read_to_end` has nothing to panic on, so `Err` here is unreachable; an empty stderr in
        // its place classifies as unrecognised output, which is a probe failure — the same answer
        // this code gives for every other thing it could not read.
        let stderr = draining_stderr.join().unwrap_or_default();
        let _ = tx.send(Written { stdout, stderr });
    });
    written
}

/// Everything a pipe holds, up to EOF or the first read error.
///
/// A read that fails mid-way keeps what arrived before it rather than discarding it: the classifier
/// reads what the child wrote, and output it cannot recognise is already a probe failure.
#[cfg(unix)]
fn read_to_end(mut pipe: impl std::io::Read) -> Vec<u8> {
    let mut bytes = Vec::new();
    let _ = pipe.read_to_end(&mut bytes);
    bytes
}

#[cfg(unix)]
impl StartedProbe {
    /// Wait for this command until `deadline`, then **end it**.
    ///
    /// The bound is on the child, not merely on this wait. A probe that only stopped waiting would
    /// leave the process running, its reader thread parked and its two pipe descriptors open, with
    /// no handle left to close any of them — and the Hosts screen probes every host it lists on
    /// every poll, so one unreachable host would leak all three per tick until the daemon ran out
    /// of descriptors.
    ///
    /// A command that ran out of time reports a failure, never a finding: nothing was read, so
    /// there is nothing this probe is entitled to say about the host.
    pub(crate) fn collect(self, deadline: std::time::Instant) -> Result<ProbeOutput, String> {
        let StartedProbe { command, running } = self;
        let running = running?;
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        match running.written.recv_timeout(remaining) {
            Ok(written) => running.finish(written),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                running.end();
                Err(format!(
                    "{} did not answer within {}s",
                    command,
                    PROBE_TIMEOUT.as_secs()
                ))
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                running.end();
                Err(format!("the {command} probe ended without reporting"))
            }
        }
    }
}

#[cfg(unix)]
impl RunningProbe {
    /// The exit status of a child that has already closed both pipes.
    ///
    /// This wait needs no bound of its own — a child at EOF on both streams is on its way out — but
    /// it is not optional: a child nobody waits for stays in the process table as a zombie.
    fn finish(mut self, written: Written) -> Result<ProbeOutput, String> {
        let status = self
            .child
            .wait()
            .map_err(|e| format!("could not read how the probe exited: {e}"))?;
        Ok(ProbeOutput {
            status,
            stdout: written.stdout,
            stderr: written.stderr,
        })
    }

    /// End a child that overran the deadline, and reap it.
    ///
    /// Both halves matter. Without the kill the process runs on; without the wait it lingers as a
    /// zombie, which is a process-table entry leaked just as surely.
    fn end(mut self) {
        if let Err(e) = self.child.kill() {
            log::warn!(
                target: "tddy_daemon::host_tooling",
                "could not end an overrunning probe: {e}"
            );
        }
        // Reaped whether or not the kill succeeded: `kill` reports an error for a child that has
        // already exited, and that child is exactly the one still waiting to be reaped.
        if let Err(e) = self.child.wait() {
            log::warn!(
                target: "tddy_daemon::host_tooling",
                "could not reap an overrunning probe: {e}"
            );
        }
    }
}
