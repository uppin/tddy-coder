use crate::handover::SocketHandover;
use crate::spawn_broker::EnvironmentBase;
use crate::spawn_broker::SpawnPlan;
use crate::spawn_broker::FORWARDED_VARIABLES;
use crate::spawn_broker::SESSION_PATH;
use crate::spawn_broker::SUPERVISOR_SOCKET_ENV;
use std::collections::BTreeMap;
use std::ffi::{CString, OsStr, OsString};
use std::os::unix::ffi::OsStrExt;
// The `extern "C" { static mut environ }` declaration lives in the parent module, beside the
// pre-exec code that also reads it; this module imports it.
use super::environ;

/// `LISTEN_PID=`, the one variable whose value the child has to fill in for itself.
const LISTEN_PID_PREFIX: &[u8] = b"LISTEN_PID=";

/// Room for every digit of a `pid_t`.
const LISTEN_PID_DIGITS: usize = 10;

/// Environment variables describing a socket activation. Removed from every child's environment
/// before anything is added back, because they only ever describe one process.
const ACTIVATION_VARIABLES: [&str; 3] = ["LISTEN_FDS", "LISTEN_PID", "LISTEN_FDNAMES"];

/// The exact environment a child execs with, assembled before the fork.
///
/// The supervisor owns the whole environment rather than letting `Command` compose it, for one
/// reason: `LISTEN_PID` must name the child, whose pid does not exist until after the fork, and
/// `Command` installs its environment only *after* the `pre_exec` closure has run — so anything the
/// closure did to the environment would be discarded. Building the array here and storing its
/// pointer in `environ` from the child is the same mechanism `Command` itself uses, minus the
/// ordering problem.
pub(super) struct ChildEnvironment {
    /// Owns the bytes the pointers below refer to. Never read after construction: keeping those
    /// buffers alive until the `exec` reads them is the whole point of the field.
    #[allow(dead_code)]
    pub(super) variables: Vec<CString>,
    /// `LISTEN_PID=…` with its digits still to be written. Present only for a child being handed a
    /// listener.
    listen_pid: Option<Box<[u8]>>,
    /// A null-terminated `envp`, exactly what `execvp` expects.
    pointers: Vec<*const libc::c_char>,
}

// SAFETY: every pointer in `pointers` refers to a buffer owned by the same struct, and nothing else
// holds a pointer into either. Moving the struct between threads moves all of it.
unsafe impl Send for ChildEnvironment {}
unsafe impl Sync for ChildEnvironment {}

impl ChildEnvironment {
    pub(super) fn build(
        plan: &SpawnPlan,
        handover: Option<&SocketHandover>,
    ) -> std::io::Result<ChildEnvironment> {
        let hands_over_listener = handover.is_some();
        let mut values: BTreeMap<OsString, OsString> = match &plan.environment {
            EnvironmentBase::Inherited { .. } => std::env::vars_os().collect(),
            EnvironmentBase::Minimal => minimal_environment(),
        };
        for (key, value) in &plan.env {
            values.insert(OsString::from(key), OsString::from(value));
        }
        // The child's own account, not the supervisor's, and after the request's own variables so a
        // caller cannot describe the child as somebody it is not. A child that kept root's `HOME`
        // would write its state where the account it runs as cannot read it, and one that kept root's
        // `USER` would tell every tool it runs that it is root.
        values.insert(
            OsString::from("HOME"),
            plan.target.home.clone().into_os_string(),
        );
        let account = OsString::from(plan.target.name.to_string_lossy().into_owned());
        values.insert(OsString::from("USER"), account.clone());
        values.insert(OsString::from("LOGNAME"), account);
        // Last, and after the request's own variables: an activation describes exactly one process,
        // so only the supervisor gets to say whether this child has one. Inherited from its own
        // activation, or asked for by a caller, these would have a child adopt whatever its
        // descriptor 3 happened to be as a listener.
        for variable in ACTIVATION_VARIABLES {
            values.remove(OsStr::new(variable));
        }
        // The same rule for the supervisor's own socket: last, and dropped for every child before
        // anything is added back, so neither a caller nor an inherited value can tell a child where
        // the supervisor listens. Only a declared managed service is told.
        values.remove(OsStr::new(SUPERVISOR_SOCKET_ENV));
        if let EnvironmentBase::Inherited { supervisor_socket } = &plan.environment {
            values.insert(
                OsString::from(SUPERVISOR_SOCKET_ENV),
                supervisor_socket.clone().into_os_string(),
            );
        }
        if let Some(handover) = handover {
            values.insert(
                OsString::from("LISTEN_FDS"),
                OsString::from(handover.count().to_string()),
            );
            values.insert(
                OsString::from("LISTEN_FDNAMES"),
                OsString::from(handover.fd_names()),
            );
        }

        let mut variables = Vec::with_capacity(values.len());
        for (key, value) in &values {
            variables.push(environment_entry(key, value)?);
        }
        let listen_pid = hands_over_listener.then(listen_pid_entry);

        let mut pointers: Vec<*const libc::c_char> =
            variables.iter().map(|entry| entry.as_ptr()).collect();
        if let Some(entry) = &listen_pid {
            pointers.push(entry.as_ptr().cast());
        }
        pointers.push(std::ptr::null());

        Ok(ChildEnvironment {
            variables,
            listen_pid,
            pointers,
        })
    }

    /// Stamp this environment with the child's own pid and install it.
    ///
    /// # Safety
    ///
    /// Runs after `fork`. Writes into memory this struct owns and stores one pointer into `environ`;
    /// it allocates nothing, takes no lock, and the array outlives the `exec` that reads it.
    pub(super) unsafe fn install(&mut self) {
        if let Some(entry) = &mut self.listen_pid {
            write_listen_pid(entry);
        }
        environ = self.pointers.as_ptr();
    }
}

/// The base environment of a session: a `PATH` it did not choose, plus the locale.
///
/// `HOME`, `USER` and `LOGNAME` are not here — [`ChildEnvironment::build`] derives them from the
/// resolved account after everything else, so no caller can set them.
fn minimal_environment() -> BTreeMap<OsString, OsString> {
    let mut values = BTreeMap::new();
    values.insert(OsString::from("PATH"), OsString::from(SESSION_PATH));
    for variable in FORWARDED_VARIABLES {
        if let Some(value) = std::env::var_os(variable) {
            values.insert(OsString::from(variable), value);
        }
    }
    values
}

fn environment_entry(key: &OsStr, value: &OsStr) -> std::io::Result<CString> {
    let mut entry = Vec::with_capacity(key.as_bytes().len() + 1 + value.as_bytes().len());
    entry.extend_from_slice(key.as_bytes());
    entry.push(b'=');
    entry.extend_from_slice(value.as_bytes());
    CString::new(entry).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "environment variable {} contains a nul byte",
                key.to_string_lossy()
            ),
        )
    })
}

/// `LISTEN_PID=` with room for the digits the child writes into it.
fn listen_pid_entry() -> Box<[u8]> {
    let mut entry = vec![0u8; LISTEN_PID_PREFIX.len() + LISTEN_PID_DIGITS + 1];
    entry[..LISTEN_PID_PREFIX.len()].copy_from_slice(LISTEN_PID_PREFIX);
    entry.into_boxed_slice()
}

/// Write this process's own pid into a `LISTEN_PID=` entry, in place.
///
/// Formatted by hand into the buffer the entry already owns: between `fork` and `exec` there is no
/// allocator to reach for, so `format!` is not available. The daemon checks this value against its
/// own pid before adopting the descriptor, which is the contract being satisfied.
fn write_listen_pid(entry: &mut [u8]) {
    // SAFETY: `getpid` only reads this process's own pid.
    let mut remaining = unsafe { libc::getpid() } as u32;
    let mut digits = [0u8; LISTEN_PID_DIGITS];
    let mut written = 0;
    loop {
        digits[written] = b'0' + (remaining % 10) as u8;
        remaining /= 10;
        written += 1;
        if remaining == 0 {
            break;
        }
    }
    let start = LISTEN_PID_PREFIX.len();
    for (offset, digit) in digits[..written].iter().rev().enumerate() {
        entry[start + offset] = *digit;
    }
    entry[start + written] = 0;
}
