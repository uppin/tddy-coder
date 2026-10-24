//! Starting a child as another OS user, with its pre-exec steps reported to the log.
//!
//! Between `fork` and `exec` a child may only make async-signal-safe calls, and it cannot log. What
//! it can do is `write(2)` to a pipe. [`StagedCommand`] gives the child a close-on-exec pipe and has
//! it write one fixed-size record per step — forked, groups set, gid set, uid set, about to exec. A
//! watcher thread in the parent reads them, logs each with the child's pid, and while a step is
//! outstanding logs `still at <step> after <n>s` every [`STAGE_HEARTBEAT`]. A successful `exec`
//! closes the pipe; end-of-file after [`PreExecStage::Exec`] is success.
//!
//! The privilege drop itself is three bare syscalls over values resolved before the fork
//! ([`crate::privilege_drop::TargetAccount`]) — no environment writes, no NSS, no allocation.

use std::time::{Duration, Instant};

#[cfg(unix)]
use std::fs::File;
#[cfg(unix)]
use std::io::{self, ErrorKind, Read};
#[cfg(unix)]
use std::os::fd::{AsRawFd, FromRawFd, RawFd};

#[cfg(unix)]
use crate::privilege_drop::{PrivilegeChange, TargetAccount};

/// A step of a child's life between `fork` and `exec`, in the order it happens.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreExecStage {
    Forked = 1,
    Groups = 2,
    Gid = 3,
    Uid = 4,
    Exec = 5,
}

impl PreExecStage {
    /// Every step, in the order a child reaches them.
    const ALL: [PreExecStage; 5] = [
        PreExecStage::Forked,
        PreExecStage::Groups,
        PreExecStage::Gid,
        PreExecStage::Uid,
        PreExecStage::Exec,
    ];

    /// The step a record's first byte names, or `None` for a byte that names none.
    pub fn from_byte(byte: u8) -> Option<PreExecStage> {
        Self::ALL.into_iter().find(|stage| *stage as u8 == byte)
    }

    /// The step's name as the log prints it: `forked`, `groups`, `gid`, `uid`, `exec`.
    pub fn name(self) -> &'static str {
        match self {
            PreExecStage::Forked => "forked",
            PreExecStage::Groups => "groups",
            PreExecStage::Gid => "gid",
            PreExecStage::Uid => "uid",
            PreExecStage::Exec => "exec",
        }
    }
}

/// Bytes in one stage record: the step, then the child's pid as a little-endian `u32`.
pub const STAGE_RECORD_LEN: usize = 5;

/// One stage record as the child writes it.
pub fn stage_record(stage: PreExecStage, pid: u32) -> [u8; STAGE_RECORD_LEN] {
    let [p0, p1, p2, p3] = pid.to_le_bytes();
    [stage as u8, p0, p1, p2, p3]
}

/// How often a step that has not finished is logged again.
pub const STAGE_HEARTBEAT: Duration = Duration::from_secs(5);

/// What the child's records said by the time its stage pipe closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageOutcome {
    /// The last record was [`PreExecStage::Exec`]: the child became its program.
    Execed,
    /// The pipe closed with this as the last step reached — the child failed or died there.
    StoppedAt(PreExecStage),
    /// The pipe closed before the child wrote anything.
    NothingReported,
}

/// The last step a child reported, with the pid it reported it under and when.
#[cfg(unix)]
#[derive(Debug, Clone, Copy)]
struct Reached {
    stage: PreExecStage,
    pid: u32,
    at: Instant,
}

/// Read stage records from `reader` until end-of-file, logging each as
/// `spawn <label> pid=<pid> reached <stage>` and, while a step is outstanding, logging
/// `spawn <label> pid=<pid> still at <stage> after <n>s` every `heartbeat`.
///
/// [`StagedCommand`] runs this on its own thread; a test drives it with a pipe it writes itself.
#[cfg(unix)]
pub fn watch_stages(mut reader: File, label: &str, heartbeat: Duration) -> StageOutcome {
    let mut pending: Vec<u8> = Vec::with_capacity(2 * STAGE_RECORD_LEN);
    let mut last: Option<Reached> = None;
    let mut deadline = Instant::now() + heartbeat;
    let mut chunk = [0u8; 4 * STAGE_RECORD_LEN];
    loop {
        match wait_readable(
            reader.as_raw_fd(),
            deadline.saturating_duration_since(Instant::now()),
        ) {
            Ok(true) => {}
            Ok(false) => {
                if let Some(reached) = last {
                    log::warn!(
                        "spawn {label} pid={} still at {} after {}s",
                        reached.pid,
                        reached.stage.name(),
                        reached.at.elapsed().as_secs()
                    );
                }
                deadline = Instant::now() + heartbeat;
                continue;
            }
            Err(e) => {
                log::warn!("spawn {label}: waiting on the stage pipe failed: {e}");
                break;
            }
        }
        let read = match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => read,
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) => {
                log::warn!("spawn {label}: reading the stage pipe failed: {e}");
                break;
            }
        };
        pending.extend_from_slice(&chunk[..read]);
        let whole = pending.len() / STAGE_RECORD_LEN * STAGE_RECORD_LEN;
        for record in pending[..whole].chunks_exact(STAGE_RECORD_LEN) {
            match decode_record(record) {
                Some((stage, pid)) => {
                    log::info!("spawn {label} pid={pid} reached {}", stage.name());
                    last = Some(Reached {
                        stage,
                        pid,
                        at: Instant::now(),
                    });
                    deadline = Instant::now() + heartbeat;
                }
                None => log::warn!("spawn {label}: unrecognised stage record {record:?}"),
            }
        }
        pending.drain(..whole);
    }
    match last {
        Some(Reached {
            stage: PreExecStage::Exec,
            ..
        }) => StageOutcome::Execed,
        Some(reached) => StageOutcome::StoppedAt(reached.stage),
        None => StageOutcome::NothingReported,
    }
}

/// One whole record as its step and pid, or `None` for a record whose first byte names no step.
#[cfg(unix)]
fn decode_record(record: &[u8]) -> Option<(PreExecStage, u32)> {
    let stage = PreExecStage::from_byte(record[0])?;
    let pid = u32::from_le_bytes([record[1], record[2], record[3], record[4]]);
    Some((stage, pid))
}

/// Wait up to `timeout` for `fd` to become readable or to reach end-of-file. `Ok(false)` is the
/// timeout. Rounded up to whole milliseconds, so a sub-millisecond remainder waits and does not spin.
#[cfg(unix)]
fn wait_readable(fd: RawFd, timeout: Duration) -> io::Result<bool> {
    let millis = timeout.as_nanos().div_ceil(1_000_000).min(i32::MAX as u128) as libc::c_int;
    let mut fds = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    loop {
        // SAFETY: `fds` is one live `pollfd`, and `poll` writes only its `revents`.
        let ready = unsafe { libc::poll(&mut fds, 1, millis) };
        if ready >= 0 {
            return Ok(ready > 0);
        }
        let err = io::Error::last_os_error();
        if err.kind() != ErrorKind::Interrupted {
            return Err(err);
        }
    }
}

/// A command to start as a target account, with its pre-exec steps reported.
#[cfg(unix)]
pub struct StagedCommand {
    command: std::process::Command,
    label: String,
    change: PrivilegeChange,
}

#[cfg(unix)]
impl StagedCommand {
    /// Wrap `command`. `label` names it in the log (`clone /home/u/repos/x`, `session 01a1…`).
    /// `change` comes from [`crate::privilege_drop::plan_privilege_change`], so a refusal has already
    /// happened before this exists.
    pub fn new(
        command: std::process::Command,
        label: &str,
        change: PrivilegeChange,
    ) -> StagedCommand {
        StagedCommand {
            command,
            label: label.to_string(),
            change,
        }
    }

    /// Install the async-signal-safe `pre_exec` (stage records + privilege drop) and spawn.
    pub fn spawn(self) -> io::Result<StagedChild> {
        use std::os::unix::process::CommandExt;

        let StagedCommand {
            mut command,
            label,
            change,
        } = self;
        let (reader, writer) = cloexec_pipe()?;
        let stage_fd = writer.as_raw_fd();
        let target = match change {
            PrivilegeChange::None => None,
            PrivilegeChange::Drop(target) => Some(target),
        };
        // SAFETY: `child_steps` runs between fork and exec, so it makes only async-signal-safe
        // calls (`getpid`, `write`, `setgroups`, `setgid`, `setuid`) and reads only values that were
        // resolved before the fork.
        unsafe {
            command.pre_exec(move || child_steps(stage_fd, target.as_ref()));
        }
        let child = command.spawn()?;
        // The child holds its own copy of the write end. Ours must close, or the watcher never sees
        // end-of-file.
        drop(writer);
        let stages = std::thread::spawn(move || watch_stages(reader, &label, STAGE_HEARTBEAT));
        Ok(StagedChild { child, stages })
    }
}

/// The child's side of the pre-exec steps. Runs after `fork`, so it makes only async-signal-safe
/// calls: a record before each step, the three privilege syscalls, and the `Exec` record last.
#[cfg(unix)]
fn child_steps(stage_fd: RawFd, target: Option<&TargetAccount>) -> io::Result<()> {
    record(stage_fd, PreExecStage::Forked)?;
    if let Some(target) = target {
        record(stage_fd, PreExecStage::Groups)?;
        // SAFETY: `groups` is a live buffer of `len` group ids, which `setgroups` only reads.
        if unsafe { libc::setgroups(target.groups.len() as _, target.groups.as_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        record(stage_fd, PreExecStage::Gid)?;
        // SAFETY: plain syscall over a value resolved before the fork.
        if unsafe { libc::setgid(target.gid) } != 0 {
            return Err(io::Error::last_os_error());
        }
        record(stage_fd, PreExecStage::Uid)?;
        // SAFETY: plain syscall over a value resolved before the fork.
        if unsafe { libc::setuid(target.uid) } != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    record(stage_fd, PreExecStage::Exec)
}

/// Write one stage record for `stage` to `stage_fd`. Only `getpid(2)` and `write(2)`.
#[cfg(unix)]
fn record(stage_fd: RawFd, stage: PreExecStage) -> io::Result<()> {
    // SAFETY: `getpid` takes no arguments and cannot fail.
    let pid = unsafe { libc::getpid() } as u32;
    let bytes = stage_record(stage, pid);
    let mut written = 0;
    while written < bytes.len() {
        // SAFETY: the pointer and length describe the unwritten tail of `bytes`, which is live.
        let n = unsafe {
            libc::write(
                stage_fd,
                bytes[written..].as_ptr().cast(),
                bytes.len() - written,
            )
        };
        if n < 0 {
            let err = io::Error::last_os_error();
            if err.kind() == ErrorKind::Interrupted {
                continue;
            }
            return Err(err);
        }
        if n == 0 {
            return Err(io::Error::from(ErrorKind::WriteZero));
        }
        written += n as usize;
    }
    Ok(())
}

/// A pipe whose ends are both close-on-exec, so the child's write end is gone once it execs.
#[cfg(unix)]
fn cloexec_pipe() -> io::Result<(File, File)> {
    let mut fds: [RawFd; 2] = [-1; 2];
    #[cfg(target_os = "linux")]
    // SAFETY: `fds` has room for the two descriptors `pipe2` writes.
    let made = unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) };
    #[cfg(not(target_os = "linux"))]
    // SAFETY: `fds` has room for the two descriptors `pipe` writes.
    let made = unsafe { libc::pipe(fds.as_mut_ptr()) };
    if made != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the pipe was created above, so both descriptors are open and owned by nobody else.
    // Owning them now means an error below closes both ends.
    let reader = unsafe { File::from_raw_fd(fds[0]) };
    let writer = unsafe { File::from_raw_fd(fds[1]) };
    #[cfg(not(target_os = "linux"))]
    {
        // macOS has no `pipe2`, so the flag is set after creation, as std's own pipes do there.
        set_close_on_exec(reader.as_raw_fd())?;
        set_close_on_exec(writer.as_raw_fd())?;
    }
    Ok((reader, writer))
}

#[cfg(all(unix, not(target_os = "linux")))]
fn set_close_on_exec(fd: RawFd) -> io::Result<()> {
    // SAFETY: `fd` is an open descriptor owned by the caller; `F_SETFD` takes an int argument.
    if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// A child started by [`StagedCommand::spawn`].
#[cfg(unix)]
pub struct StagedChild {
    pub child: std::process::Child,
    /// The watcher; joins with what the child's records said.
    pub stages: std::thread::JoinHandle<StageOutcome>,
}
