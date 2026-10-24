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

use std::time::Duration;

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
    /// The step a record's first byte names, or `None` for a byte that names none.
    pub fn from_byte(byte: u8) -> Option<PreExecStage> {
        // TODO(spawn-diag): implement
        let _ = byte;
        None
    }

    /// The step's name as the log prints it: `forked`, `groups`, `gid`, `uid`, `exec`.
    pub fn name(self) -> &'static str {
        // TODO(spawn-diag): implement
        ""
    }
}

/// Bytes in one stage record: the step, then the child's pid as a little-endian `u32`.
pub const STAGE_RECORD_LEN: usize = 5;

/// One stage record as the child writes it.
pub fn stage_record(stage: PreExecStage, pid: u32) -> [u8; STAGE_RECORD_LEN] {
    // TODO(spawn-diag): implement
    let _ = (stage, pid);
    [0; STAGE_RECORD_LEN]
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

/// Read stage records from `reader` until end-of-file, logging each as
/// `spawn <label> pid=<pid> reached <stage>` and, while a step is outstanding, logging
/// `spawn <label> pid=<pid> still at <stage> after <n>s` every `heartbeat`.
///
/// [`StagedCommand`] runs this on its own thread; a test drives it with a pipe it writes itself.
#[cfg(unix)]
pub fn watch_stages(reader: std::fs::File, label: &str, heartbeat: Duration) -> StageOutcome {
    // TODO(spawn-diag): implement — poll(2) with `heartbeat` timeout, 5-byte records, log::info!/warn!
    let _ = (reader, label, heartbeat);
    StageOutcome::NothingReported
}

/// A command to start as a target account, with its pre-exec steps reported.
#[cfg(unix)]
pub struct StagedCommand {
    command: std::process::Command,
    label: String,
    change: crate::privilege_drop::PrivilegeChange,
}

#[cfg(unix)]
impl StagedCommand {
    /// Wrap `command`. `label` names it in the log (`clone /home/u/repos/x`, `session 01a1…`).
    /// `change` comes from [`crate::privilege_drop::plan_privilege_change`], so a refusal has already
    /// happened before this exists.
    pub fn new(
        command: std::process::Command,
        label: &str,
        change: crate::privilege_drop::PrivilegeChange,
    ) -> StagedCommand {
        StagedCommand {
            command,
            label: label.to_string(),
            change,
        }
    }

    /// Install the async-signal-safe `pre_exec` (stage records + privilege drop) and spawn.
    pub fn spawn(self) -> std::io::Result<StagedChild> {
        // TODO(spawn-diag): implement
        let _ = (self.command, self.label, self.change);
        Err(std::io::Error::other(
            "StagedCommand::spawn: not implemented",
        ))
    }
}

/// A child started by [`StagedCommand::spawn`].
#[cfg(unix)]
pub struct StagedChild {
    pub child: std::process::Child,
    /// The watcher; joins with what the child's records said.
    pub stages: std::thread::JoinHandle<StageOutcome>,
}
