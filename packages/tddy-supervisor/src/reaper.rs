//! Reaping exited children.
//!
//! The supervisor is the parent of every managed service and every session it spawns, so it is the
//! only process that can reap them. `SIGCHLD` is coalescing — several children exiting close
//! together produce one signal — so every notification drains the whole queue rather than reaping
//! once.

/// A child the kernel has finished with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReapedChild {
    pub pid: u32,
    /// Raw wait status, as reported by `waitpid`.
    pub status: i32,
}

impl ReapedChild {
    /// The code the child exited with, or `None` when a signal ended it instead.
    ///
    /// A signalled child genuinely has no exit code; reporting the signal number as one would tell a
    /// caller the process chose to exit with a status it never chose.
    pub fn exit_code(&self) -> Option<i32> {
        if libc::WIFEXITED(self.status) {
            Some(libc::WEXITSTATUS(self.status))
        } else {
            None
        }
    }

    /// Human-readable exit description for the log line that attributes it to a service.
    pub fn describe(&self) -> String {
        if libc::WIFEXITED(self.status) {
            format!("exit status {}", libc::WEXITSTATUS(self.status))
        } else if libc::WIFSIGNALED(self.status) {
            format!("signal {}", libc::WTERMSIG(self.status))
        } else {
            format!("wait status {}", self.status)
        }
    }
}

/// Reap every child that has already exited, without blocking.
///
/// Returns them in the order the kernel hands them over. A child that exited before this call is
/// reported exactly once; a child still running is not reported at all.
pub fn reap_exited_children() -> Vec<ReapedChild> {
    let mut reaped = Vec::new();
    loop {
        let mut status: libc::c_int = 0;
        // SAFETY: `waitpid` with -1 and WNOHANG only inspects this process's own children and
        // returns immediately. `status` is a live local.
        let pid = unsafe { libc::waitpid(-1, &mut status, libc::WNOHANG) };
        // 0 means children exist but none have exited; -1 means none are left (or EINTR, which
        // WNOHANG makes moot). Either way there is nothing more to collect right now.
        if pid <= 0 {
            return reaped;
        }
        reaped.push(ReapedChild {
            pid: pid as u32,
            status,
        });
    }
}
