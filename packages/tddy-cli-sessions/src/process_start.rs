//! When a process started — what tells a pid read earlier from whatever process the kernel gave
//! that pid to since (`#tab-hardening` 2/4). Read through `sysinfo`, so it is the same on macOS
//! and Linux; its resolution is the OS's start time in whole seconds.

/// When a process started, in whole seconds since the Unix epoch. Two readings of one process are
/// equal; a pid whose reading changed names another process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProcessStart(u64);

impl ProcessStart {
    /// The start a tmux main's roster node keeps (`tddy_core::tab_roster::START_PROPERTY`).
    #[must_use]
    pub const fn from_epoch_secs(secs: u64) -> Self {
        Self(secs)
    }

    /// Seconds since the Unix epoch, as the roster keeps them.
    #[must_use]
    pub const fn epoch_secs(self) -> u64 {
        self.0
    }
}

/// When the process `pid` started, or `None` when no process has that pid.
#[must_use]
pub fn process_start(pid: u32) -> Option<ProcessStart> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

    let pid = Pid::from_u32(pid);
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing(),
    );
    system
        .process(pid)
        .map(|process| ProcessStart::from_epoch_secs(process.start_time()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pid no process holds: a child that has been reaped.
    fn a_pid_with_no_process() -> u32 {
        let mut child = std::process::Command::new("true").spawn().unwrap();
        let pid = child.id();
        child.wait().unwrap();
        pid
    }

    #[test]
    fn a_live_process_has_a_start() {
        // Given
        let pid = std::process::id();

        // When
        let start = process_start(pid);

        // Then
        assert!(start.is_some(), "this test process has a start: {start:?}");
    }

    #[test]
    fn a_process_reads_the_same_start_each_time() {
        // Given
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let pid = child.id();

        // When
        let readings = (process_start(pid), process_start(pid));
        child.kill().unwrap();
        child.wait().unwrap();

        // Then
        assert!(readings.0.is_some(), "the child has a start");
        assert_eq!(readings.0, readings.1);
    }

    #[test]
    fn a_process_started_now_reads_a_start_near_now() {
        // Given
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();

        // When
        let start = process_start(child.id());
        child.kill().unwrap();
        child.wait().unwrap();

        // Then
        let secs = start.expect("the child has a start").epoch_secs();
        assert!(
            secs + 2 >= now && secs <= now + 2,
            "started at {secs}, now is {now}"
        );
    }

    #[test]
    fn a_pid_with_no_process_has_no_start() {
        // Given
        let pid = a_pid_with_no_process();

        // When
        let start = process_start(pid);

        // Then
        assert_eq!(start, None);
    }
}
