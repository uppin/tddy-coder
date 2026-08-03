//! Sending signals to the processes the supervisor owns.
//!
//! Kept as one narrow seam so the shutdown sequence in [`crate::supervisor`] reads as policy
//! (terminate, wait out the grace period, kill survivors) rather than as `unsafe` blocks.

/// Ask a process to exit.
pub const TERMINATE: i32 = libc::SIGTERM;

/// Take a process out that did not exit on request.
pub const KILL: i32 = libc::SIGKILL;

/// Send `signal` to `pid`.
///
/// A pid the kernel no longer knows is not an error: between deciding to signal a child and doing
/// it, the child may have exited on its own. That race is expected, not exceptional.
pub fn signal_process(pid: u32, signal: i32) {
    // SAFETY: `kill` only inspects the pid table. The caller passes a pid the supervisor has
    // spawned and not yet reaped, so it has not been recycled.
    let sent = unsafe { libc::kill(pid as i32, signal) };
    report(sent, &format!("pid {pid}"), signal);
}

/// Send `signal` to the process group `pid` leads.
///
/// Reaches the descendants a session started for itself, which a signal to the pid alone would miss.
/// Only ever called for a pid the supervisor spawned: every child it forks calls `setsid` and so
/// leads a group of its own, which is what keeps this from reaching the supervisor's own group.
pub fn signal_process_group(pid: u32, signal: i32) {
    // SAFETY: a negative pid addresses the process group with that id. The caller passes a pid the
    // supervisor spawned and has not yet reaped, so neither it nor its group id has been recycled.
    let sent = unsafe { libc::kill(-(pid as i32), signal) };
    report(sent, &format!("process group {pid}"), signal);
}

/// A pid or group the kernel no longer knows is not an error: between deciding to signal a child and
/// doing it, the child may have exited on its own.
fn report(sent: libc::c_int, target: &str, signal: i32) {
    if sent != 0 {
        let error = std::io::Error::last_os_error();
        log::debug!(
            target: "tddy_supervisor::signals",
            "signal {signal} to {target} was not delivered: {error}"
        );
    }
}
