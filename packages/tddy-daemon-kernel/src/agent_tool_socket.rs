//! Where an embedded daemon's agent-facing tool socket lives: one path **per instance**.
//!
//! [`crate::agent_tool_sockets::AgentToolSockets`] binds it the first time a session needs it and
//! hands that very path to the session, so nothing else ever derives it.
//!
//! **Per instance, not per data dir.** Two Tddy Desktop instances may run on one data dir — a
//! second one is started by anything that runs the application binary — and each serves its own
//! socket. A path derived from the data dir alone was shared by both: the second instance unlinked
//! the first's socket to bind its own and, once it exited, left a dead file there, so every session
//! of the first was refused (`Connection refused`) until it restarted.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const SOCKET_PREFIX: &str = "tddy-agent-tools-";
const SOCKET_SUFFIX: &str = ".sock";

/// A fresh path for this process's next embedded agent-tool socket:
/// `$TMPDIR/tddy-agent-tools-<pid>-<n>.sock`.
///
/// Under `$TMPDIR` rather than beside the session data, because an `AF_UNIX` path is limited to
/// about 104 bytes on macOS and a checkout path alone can spend most of that. The pid is what lets a
/// later instance tell the socket of an instance that is gone ([`clear_dead_instances_sockets`]);
/// `n` tells apart the sockets of one process — one bound again after its file was removed, or two
/// hosts in one test binary.
pub fn next_embedded_socket_path() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    instance_socket_path(std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed))
}

/// The `n`th embedded socket path of the process `pid`.
pub(crate) fn instance_socket_path(pid: u32, n: u64) -> PathBuf {
    socket_dir().join(format!("{SOCKET_PREFIX}{pid}-{n}{SOCKET_SUFFIX}"))
}

fn socket_dir() -> PathBuf {
    std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
}

/// Remove the embedded sockets in `$TMPDIR` whose instance's process has exited — what an instance
/// killed before it could remove its own leaves behind. A socket of a live process, one another
/// account owns, and one something still listens on are left where they are.
pub fn clear_dead_instances_sockets() {
    clear_dead_instances_sockets_on(&tddy_fs::DiskFs)
}

/// [`clear_dead_instances_sockets`] through the filesystem port.
pub fn clear_dead_instances_sockets_on(fs: &dyn tddy_fs::Fs) {
    let Ok(entries) = fs.read_dir(&socket_dir()) else {
        return;
    };
    let this_process = std::process::id();
    // SAFETY: reads this process's own credentials.
    let this_account = unsafe { libc::geteuid() };
    for path in entries {
        let Some(pid) = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(instance_pid)
        else {
            continue;
        };
        if pid == this_process || process_is_alive(pid) {
            continue;
        }
        if let Err(e) = crate::owner_socket::clear_stale_socket_on(fs, &path, &[this_account]) {
            log::debug!(
                target: "tddy_daemon_kernel::agent_tool_socket",
                "left {} in place: {e:#}",
                path.display()
            );
        }
    }
}

/// The pid an embedded socket's file name carries, when `file_name` is one.
fn instance_pid(file_name: &str) -> Option<u32> {
    let rest = file_name
        .strip_prefix(SOCKET_PREFIX)?
        .strip_suffix(SOCKET_SUFFIX)?;
    let (pid, n) = rest.split_once('-')?;
    n.parse::<u64>().ok()?;
    pid.parse().ok().filter(|pid| *pid > 0)
}

/// Whether a process `pid` exists, as anybody: `kill(pid, 0)` fails with `ESRCH` only when there is
/// no such process, and with `EPERM` for one that belongs to another account.
fn process_is_alive(pid: u32) -> bool {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return false;
    };
    // SAFETY: signal 0 sends nothing; it only checks that `pid` can be addressed.
    if unsafe { libc::kill(pid, 0) } == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_socket_path_is_short_enough_for_af_unix_whatever_the_pid() {
        // Given the longest pid either platform hands out (Linux's `pid_max` is 2^22) and a
        // counter far past anything one process reaches
        let path = instance_socket_path(4_194_304, 99_999);

        assert!(
            path.as_os_str().len() < 104,
            "AF_UNIX paths are capped near 104 bytes; got {} ({})",
            path.as_os_str().len(),
            path.display()
        );
    }

    #[test]
    fn every_socket_of_one_process_has_a_path_of_its_own() {
        let first = next_embedded_socket_path();
        let second = next_embedded_socket_path();

        assert_ne!(
            first, second,
            "a shared path would cross two hosts' sessions"
        );
    }

    #[test]
    fn a_socket_path_names_the_process_that_binds_it() {
        let path = next_embedded_socket_path();

        assert_eq!(
            path.file_name()
                .and_then(|name| name.to_str())
                .and_then(instance_pid),
            Some(std::process::id())
        );
    }

    #[test]
    fn only_an_embedded_socket_name_carries_a_pid() {
        assert_eq!(
            [
                "tddy-agent-tools-123-0.sock",
                "tddy-agent-tools-cafdb3277ee051d2.sock",
                "tddy-agent-tools-0-0.sock",
                "tddy-agent-tools-123-x.sock",
                "tddy-index-daemon.sock",
            ]
            .map(instance_pid),
            [Some(123), None, None, None, None]
        );
    }
}
