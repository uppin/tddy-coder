//! Handing the supervisor's listening sockets to a managed service it starts.
//!
//! A service's own listener goes to descriptor 3 and each per-OS-user host socket (see
//! [`crate::host_socket`]) to the descriptor after it, announced in `LISTEN_FDS` and
//! `LISTEN_FDNAMES` (see [`crate::socket`]). The listeners are gathered before the fork; the
//! descriptor placement runs between `fork` and `exec`, where only async-signal-safe calls are
//! allowed.

use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::net::UnixListener;
use std::sync::Arc;

use crate::socket::{
    host_session_fd_name, MAX_HANDED_OVER_LISTENERS, SD_LISTEN_FDS_START, SERVICE_SOCKET_FD_NAME,
};

/// The listening sockets the supervisor created for a managed service, ready to be handed over:
/// the service's own at descriptor 3, then one per OS user it was declared a host socket for.
///
/// The supervisor keeps the listeners for as long as it runs, so a restarted service is handed the
/// same ones. Rebinding per start would unlink and recreate the socket node, and a client that
/// connected in that gap would get `ECONNREFUSED` on a path that is about to work again; holding the
/// listener keeps the kernel's accept queue instead, so those connections simply wait. It is the
/// same bargain systemd's socket activation makes.
#[derive(Debug, Clone)]
pub struct SocketHandover {
    /// `LISTEN_FDNAMES` entry and listener, in descriptor order starting at [`SD_LISTEN_FDS_START`].
    listeners: Vec<(String, Arc<UnixListener>)>,
}

impl SocketHandover {
    /// A handover of the service's own listener alone.
    pub fn new(listener: Arc<UnixListener>) -> SocketHandover {
        SocketHandover {
            listeners: vec![(SERVICE_SOCKET_FD_NAME.to_string(), listener)],
        }
    }

    /// Also hand over `os_user`'s host socket, at the next descriptor and under its own name.
    pub fn with_host_session_socket(
        mut self,
        os_user: &str,
        listener: Arc<UnixListener>,
    ) -> SocketHandover {
        self.listeners
            .push((host_session_fd_name(os_user), listener));
        self
    }

    /// The descriptors to place at [`SD_LISTEN_FDS_START`] and after, in order. Borrowed, never
    /// owned: the listeners outlive every child that is handed them.
    pub(crate) fn raw_fds(&self) -> Vec<RawFd> {
        self.listeners
            .iter()
            .map(|(_, listener)| listener.as_raw_fd())
            .collect()
    }

    /// How many descriptors are handed over: `LISTEN_FDS`.
    pub(crate) fn count(&self) -> usize {
        self.listeners.len()
    }

    /// `LISTEN_FDNAMES`: one colon-separated name per descriptor, in descriptor order.
    pub(crate) fn fd_names(&self) -> String {
        self.listeners
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>()
            .join(":")
    }
}

/// Make sure every descriptor a handover can use — [`SD_LISTEN_FDS_START`] up to
/// [`MAX_HANDED_OVER_LISTENERS`] of them — is occupied, so nothing the standard library opens can
/// land on one.
///
/// A declared listener reaches the child with `dup2`, which silently replaces whatever is already
/// there. `Command::spawn` reports exec failures over a socket pair it opens just before the fork,
/// and if that descriptor were a handover slot then an exec that failed would look to the
/// supervisor like one that succeeded. The kernel hands out the lowest free descriptor, so occupying
/// the slots from startup — in ascending order, which is what makes each `open` land on the slot it
/// is meant for — is what makes that impossible.
///
/// A slot something already occupies — the tokio runtime's epoll descriptor normally takes the
/// first — needs nothing: the invariant already holds for it.
pub(crate) fn reserve_handover_slots() -> std::io::Result<Vec<OwnedFd>> {
    let mut reserved = Vec::new();
    for slot in SD_LISTEN_FDS_START..SD_LISTEN_FDS_START + MAX_HANDED_OVER_LISTENERS as RawFd {
        // SAFETY: `F_GETFD` only reads a descriptor's flags, and reports `EBADF` for a free one.
        if unsafe { libc::fcntl(slot, libc::F_GETFD) } >= 0 {
            continue;
        }
        // SAFETY: the path is a literal C string; the call returns a descriptor nothing else owns.
        let opened = unsafe { libc::open(c"/dev/null".as_ptr(), libc::O_RDONLY | libc::O_CLOEXEC) };
        if opened < 0 {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: `opened` was just returned by `open` and is not owned by anything else.
        reserved.push(unsafe { OwnedFd::from_raw_fd(opened) });
    }
    Ok(reserved)
}

/// Put the listeners the supervisor created where an activated service looks for them: the first at
/// [`SD_LISTEN_FDS_START`], the next after it, and so on.
///
/// Each is first copied above the slots, and only then onto its slot: one listener's descriptor may
/// be another's slot, and copying straight onto the slots would replace a listener not yet placed.
///
/// # Safety
///
/// Runs after `fork`. Every descriptor in `listeners` must be open in this process, and there must be
/// at most [`MAX_HANDED_OVER_LISTENERS`]. Only this process's own descriptor table is touched, and
/// nothing is allocated.
pub(crate) unsafe fn hand_over_listeners(listeners: &[RawFd]) -> std::io::Result<()> {
    let slots_end = SD_LISTEN_FDS_START + listeners.len() as RawFd;
    let mut staged = [-1 as RawFd; MAX_HANDED_OVER_LISTENERS];
    for (index, listener) in listeners.iter().enumerate() {
        // Above every slot, so no later copy can land on it; close-on-exec, so the staging copy
        // vanishes at exec instead of being inherited.
        staged[index] = libc::fcntl(*listener, libc::F_DUPFD_CLOEXEC, slots_end);
        if staged[index] < 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    for (index, copy) in staged.iter().take(listeners.len()).enumerate() {
        // `dup2` leaves `FD_CLOEXEC` clear on the slot, which is what carries the listener through
        // the exec; the inherited originals keep the flag and close themselves.
        if libc::dup2(*copy, SD_LISTEN_FDS_START + index as RawFd) < 0 {
            return Err(std::io::Error::last_os_error());
        }
        libc::close(*copy);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `(dev, ino)` of an open descriptor.
    // `st_dev` and `st_ino` are different widths on Darwin and Linux; the casts spell one type.
    #[allow(clippy::unnecessary_cast)]
    fn identity_of(fd: RawFd) -> Option<(u64, u64)> {
        // SAFETY: `stat` is written in full when `fstat` succeeds.
        let mut stat: libc::stat = unsafe { std::mem::zeroed() };
        (unsafe { libc::fstat(fd, &mut stat) } == 0)
            .then_some((stat.st_dev as u64, stat.st_ino as u64))
    }

    /// Run `check` in a forked child and report whether it returned `true`. The child is the process
    /// the handover would really run in: it may rearrange its own descriptor table freely.
    ///
    /// `check` must stick to what is safe after a `fork` of a multi-threaded process — no allocation.
    fn in_a_forked_child(check: impl FnOnce() -> bool) -> bool {
        // SAFETY: the child runs `check`, then `_exit`s; it never returns into the test harness.
        let pid = unsafe { libc::fork() };
        if pid == 0 {
            let verdict = check();
            // SAFETY: leaves the child without running the parent's destructors or atexit hooks.
            unsafe { libc::_exit(if verdict { 0 } else { 1 }) };
        }
        let mut status = 0;
        // SAFETY: `pid` is the child just forked.
        unsafe { libc::waitpid(pid, &mut status, 0) };
        libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0
    }

    /// Whether descriptor `slot` is `want`, and would survive an `exec`.
    fn slot_holds(slot: RawFd, want: (u64, u64)) -> bool {
        identity_of(slot) == Some(want)
            // SAFETY: reads the flags of a descriptor.
            && unsafe { libc::fcntl(slot, libc::F_GETFD) } == 0
    }

    #[test]
    fn places_each_listener_at_its_own_slot() {
        // Given three listeners at whatever descriptors the test process holds them at
        let directory = tempfile::TempDir::new().expect("create a directory");
        let held: Vec<UnixListener> = ["a", "b", "c"]
            .iter()
            .map(|name| UnixListener::bind(directory.path().join(name)).expect("bind"))
            .collect();
        let fds: Vec<RawFd> = held.iter().map(AsRawFd::as_raw_fd).collect();
        let want: Vec<(u64, u64)> = fds.iter().map(|fd| identity_of(*fd).unwrap()).collect();

        // When a child has them handed over, then Then each slot holds exactly its own listener
        let placed = in_a_forked_child(|| {
            // SAFETY: `fds` are open in the child, inherited across the fork.
            unsafe { hand_over_listeners(&fds) }.is_ok()
                && slot_holds(SD_LISTEN_FDS_START, want[0])
                && slot_holds(SD_LISTEN_FDS_START + 1, want[1])
                && slot_holds(SD_LISTEN_FDS_START + 2, want[2])
        });
        assert!(
            placed,
            "a listener is missing from its slot or would not survive exec"
        );
    }

    #[test]
    fn places_listeners_that_sit_in_each_others_slots() {
        // Given two listeners the child finds crossed over: the first at descriptor 4, the second at
        // descriptor 3 — so copying the first onto slot 3 would destroy the second before it moved
        let directory = tempfile::TempDir::new().expect("create a directory");
        let first = UnixListener::bind(directory.path().join("a")).expect("bind");
        let second = UnixListener::bind(directory.path().join("b")).expect("bind");
        let (first_id, second_id) = (
            identity_of(first.as_raw_fd()).unwrap(),
            identity_of(second.as_raw_fd()).unwrap(),
        );

        // When the child is handed them in declaration order
        let placed = in_a_forked_child(|| {
            // SAFETY: descriptor-table surgery on the child's own table with open descriptors; the
            // copies above 100 keep the originals out of the way of the crossing.
            let handed_over = unsafe {
                let high_first = libc::fcntl(first.as_raw_fd(), libc::F_DUPFD, 100);
                let high_second = libc::fcntl(second.as_raw_fd(), libc::F_DUPFD, 100);
                libc::dup2(high_first, 4);
                libc::dup2(high_second, 3);
                hand_over_listeners(&[4, 3]).is_ok()
            };
            handed_over
                && slot_holds(SD_LISTEN_FDS_START, first_id)
                && slot_holds(SD_LISTEN_FDS_START + 1, second_id)
        });

        // Then each ended up where its position says, not where it happened to be
        assert!(placed, "a crossed listener was clobbered or misplaced");
    }
}
