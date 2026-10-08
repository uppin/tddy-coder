//! Creating the per-OS-user host sockets a managed service declared.
//!
//! The daemon serves a socket per OS user for that user's tool sessions (`spawn_conversation`,
//! `github_token`). As an unprivileged child it cannot `chown` one to somebody else, so the root
//! supervisor makes each of them and hands the daemon the listener — the same way it hands over the
//! daemon's own socket, at the descriptors after it (see [`crate::socket`]).
//!
//! **Who can connect.** The socket is owned by the declared user, mode `0600`, in a directory owned
//! by that user, mode `0700`. Connecting needs search permission on the directory and write
//! permission on the socket, so only that user — and root — can. The daemon, an unprivileged
//! account of its own, is not among them; it does not need to be, because accepting on a listener it
//! was handed checks no file mode at all.
//!
//! **No window.** The directory is taken back by the supervisor (root, `0700`) and the socket is
//! bound, restricted and given to the user *inside* it; the directory goes to the user last. While
//! the supervisor works, nothing the user (or anyone else) controls is in the path, which is also
//! what makes `chmod`/`chown` on the path safe: there is no symlink to be swapped in for them to
//! follow. For the same reason every directory *above* it must be writable by nobody but root —
//! somebody who could replace one could aim the supervisor's `chown` at any file on the host.

use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::net::UnixListener;
use std::path::Path;

use anyhow::Context;

use crate::config::HostSocket;
use crate::spawn_broker;

/// Mode of a host socket's directory, and of the socket inside it: the user, nobody else.
const DIRECTORY_MODE: u32 = 0o700;
const SOCKET_MODE: u32 = 0o600;

/// Create `socket`'s directory and socket for its user, bound and listening. See the module docs.
pub fn bind_host_socket(service: &str, socket: &HostSocket) -> anyhow::Result<UnixListener> {
    let target = spawn_broker::resolve_target_user(&socket.user).map_err(|error| {
        anyhow::anyhow!(
            "service `{service}`: host socket user `{}`: {error}",
            socket.user
        )
    })?;
    let directory = socket
        .path
        .parent()
        .context("a host socket path has a directory")?;
    let above = directory
        .parent()
        .context("a host socket directory has a parent")?;
    // SAFETY: read this process's own credentials.
    let (euid, egid) = unsafe { (libc::geteuid(), libc::getegid()) };

    std::fs::create_dir_all(above).with_context(|| format!("create {}", above.display()))?;
    require_only_ours_can_write(above, euid)?;
    let created = match std::fs::DirBuilder::new()
        .mode(DIRECTORY_MODE)
        .create(directory)
    {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => false,
        Err(error) => {
            return Err(anyhow::Error::new(error).context(format!("create {}", directory.display())))
        }
    };
    let result = prepare(
        &socket.path,
        directory,
        (euid, egid),
        (target.uid, target.gid),
    );
    if result.is_err() && created {
        let _ = std::fs::remove_dir(directory);
    }
    let listener = result.with_context(|| {
        format!(
            "service `{service}`: host socket {} for `{}`",
            socket.path.display(),
            socket.user
        )
    })?;
    log::info!(
        target: "tddy_supervisor::host_socket",
        "created {} for `{}` (socket {SOCKET_MODE:o}, directory {DIRECTORY_MODE:o}) for service '{service}'",
        socket.path.display(),
        socket.user
    );
    Ok(listener)
}

/// Everything after the directory exists: reclaim it, bind inside it, give both to the user.
fn prepare(
    path: &Path,
    directory: &Path,
    (euid, egid): (u32, u32),
    (uid, gid): (u32, u32),
) -> anyhow::Result<UnixListener> {
    // By descriptor, not by path: `O_NOFOLLOW | O_DIRECTORY` refuses a link or a file standing in
    // for the directory, and every change below goes to the object that was checked.
    let handle = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
        .open(directory)
        .with_context(|| format!("{} is not a plain directory", directory.display()))?;
    let owner = handle.metadata()?.uid();
    if owner != euid && owner != uid {
        anyhow::bail!(
            "{} is owned by uid {owner}, who is neither the supervisor nor the socket's user",
            directory.display()
        );
    }
    // Take it back first: while it is the user's they could be racing the steps below.
    change_owner(&handle, euid, egid)?;
    handle
        .set_permissions(std::fs::Permissions::from_mode(DIRECTORY_MODE))
        .context("restrict the directory to its owner")?;

    clear_stale_socket(path)?;
    let listener = UnixListener::bind(path).with_context(|| format!("bind {}", path.display()))?;
    let finish = || -> anyhow::Result<()> {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(SOCKET_MODE))
            .context("restrict the socket to its owner")?;
        lchown(path, uid, gid)?;
        // Last: the user gets the directory only once everything in it is theirs.
        change_owner(&handle, uid, gid)
    };
    if let Err(error) = finish() {
        let _ = std::fs::remove_file(path);
        return Err(error);
    }
    Ok(listener)
}

/// Refuse a directory — and any above it — that somebody other than this process's own account
/// could write to, unless it is sticky (the kernel's own `/tmp` rule).
fn require_only_ours_can_write(directory: &Path, euid: u32) -> anyhow::Result<()> {
    let resolved = std::fs::canonicalize(directory)
        .with_context(|| format!("resolve {}", directory.display()))?;
    for ancestor in resolved.ancestors() {
        let meta =
            std::fs::metadata(ancestor).with_context(|| format!("stat {}", ancestor.display()))?;
        let sticky = meta.mode() & 0o1000 != 0;
        if meta.uid() != 0 && meta.uid() != euid {
            anyhow::bail!(
                "{} is owned by uid {}, who could replace the path the supervisor chowns through",
                ancestor.display(),
                meta.uid()
            );
        }
        if meta.mode() & 0o022 != 0 && !sticky {
            anyhow::bail!(
                "{} is writable by its group or by everyone (mode {:o}), so the path to the host \
                 socket could be redirected",
                ancestor.display(),
                meta.mode() & 0o7777
            );
        }
    }
    Ok(())
}

/// Remove a file a previous run left at the socket's path — and only a socket.
fn clear_stale_socket(path: &Path) -> anyhow::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_socket() => std::fs::remove_file(path)
            .with_context(|| format!("remove the stale {}", path.display())),
        Ok(_) => anyhow::bail!(
            "{} exists and is not a socket; not removing it",
            path.display()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(anyhow::Error::new(error).context(format!("stat {}", path.display()))),
    }
}

fn change_owner(handle: &std::fs::File, uid: u32, gid: u32) -> anyhow::Result<()> {
    // SAFETY: `handle` is an open descriptor for the duration of the call.
    if unsafe { libc::fchown(handle.as_raw_fd(), uid, gid) } != 0 {
        return Err(anyhow::Error::new(std::io::Error::last_os_error())
            .context(format!("give the directory to uid {uid}")));
    }
    Ok(())
}

/// `chown` that never follows a link: what is changed is the entry at `path` itself.
fn lchown(path: &Path, uid: u32, gid: u32) -> anyhow::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes())
        .context("a host socket path has no nul byte")?;
    // SAFETY: `c_path` outlives the call.
    if unsafe { libc::lchown(c_path.as_ptr(), uid, gid) } != 0 {
        return Err(anyhow::Error::new(std::io::Error::last_os_error())
            .context(format!("give {} to uid {uid}", path.display())));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// The account the tests run as — the one user whose ownership a non-root test can assert.
    fn this_user() -> String {
        // SAFETY: `getpwuid` returns a pointer into static storage that is read at once.
        unsafe {
            let entry = libc::getpwuid(libc::geteuid());
            assert!(!entry.is_null(), "the test account has a passwd entry");
            std::ffi::CStr::from_ptr((*entry).pw_name)
                .to_string_lossy()
                .into_owned()
        }
    }

    fn a_workspace() -> tempfile::TempDir {
        tempfile::TempDir::new().expect("create a workspace")
    }

    /// `<workspace>/run/<user>/host.sock` for this account.
    fn a_host_socket(workspace: &Path) -> HostSocket {
        HostSocket {
            user: this_user(),
            path: workspace.join("run").join(this_user()).join("host.sock"),
        }
    }

    fn mode_of(path: &Path) -> u32 {
        std::fs::symlink_metadata(path)
            .expect("stat")
            .permissions()
            .mode()
            & 0o7777
    }

    #[test]
    fn creates_a_socket_owned_by_the_user_and_closed_to_everybody_else() {
        // Given
        let workspace = a_workspace();
        let socket = a_host_socket(workspace.path());

        // When
        let _listener = bind_host_socket("tddy-daemon", &socket).expect("bind the host socket");

        // Then — owner, type and mode of both the socket and the directory it lives in
        let file = std::fs::symlink_metadata(&socket.path).expect("stat the socket");
        assert!(file.file_type().is_socket());
        assert_eq!(file.uid(), unsafe { libc::geteuid() });
        assert_eq!(mode_of(&socket.path), 0o600);
        let directory = socket.path.parent().unwrap();
        assert_eq!(std::fs::metadata(directory).unwrap().uid(), unsafe {
            libc::geteuid()
        });
        assert_eq!(mode_of(directory), 0o700);
    }

    #[test]
    fn hands_back_a_listener_that_accepts_connections() {
        // Given
        let workspace = a_workspace();
        let socket = a_host_socket(workspace.path());
        let listener = bind_host_socket("tddy-daemon", &socket).expect("bind the host socket");

        // When the user connects
        let _client = std::os::unix::net::UnixStream::connect(&socket.path).expect("connect");

        // Then the listener the supervisor kept is the one that accepts it
        listener.accept().expect("accept the connection");
    }

    #[test]
    fn narrows_a_directory_that_was_left_wider() {
        // Given a directory from an earlier install that anybody can list
        let workspace = a_workspace();
        let socket = a_host_socket(workspace.path());
        let directory = socket.path.parent().unwrap();
        std::fs::create_dir_all(directory).unwrap();
        std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o755)).unwrap();

        // When
        let _listener = bind_host_socket("tddy-daemon", &socket).expect("bind the host socket");

        // Then
        assert_eq!(mode_of(directory), 0o700);
    }

    #[test]
    fn replaces_a_socket_a_previous_run_left_behind() {
        // Given a dead supervisor's socket file
        let workspace = a_workspace();
        let socket = a_host_socket(workspace.path());
        drop(bind_host_socket("tddy-daemon", &socket).expect("first bind"));
        assert!(socket.path.exists());

        // When
        let listener = bind_host_socket("tddy-daemon", &socket).expect("bind again");

        // Then it is a live listener again
        let _client = std::os::unix::net::UnixStream::connect(&socket.path).expect("connect");
        listener.accept().expect("accept the connection");
    }

    #[test]
    fn never_removes_a_file_that_is_not_a_socket() {
        // Given a regular file where the socket belongs
        let workspace = a_workspace();
        let socket = a_host_socket(workspace.path());
        std::fs::create_dir_all(socket.path.parent().unwrap()).unwrap();
        std::fs::write(&socket.path, "precious").unwrap();

        // When
        let error = bind_host_socket("tddy-daemon", &socket).expect_err("refused");

        // Then
        assert!(format!("{error:#}").contains("not a socket"), "{error:#}");
        assert_eq!(std::fs::read_to_string(&socket.path).unwrap(), "precious");
    }

    #[test]
    fn refuses_an_account_the_host_does_not_have_and_creates_nothing() {
        // Given
        let workspace = a_workspace();
        let socket = HostSocket {
            user: "no-such-account-tddy".to_string(),
            path: workspace.path().join("run/ghost/host.sock"),
        };

        // When
        let error = bind_host_socket("tddy-daemon", &socket).expect_err("refused");

        // Then
        assert!(
            format!("{error:#}").contains("no-such-account-tddy"),
            "{error:#}"
        );
        assert!(!workspace.path().join("run").exists());
    }

    #[test]
    fn refuses_a_directory_that_is_a_symlink_and_leaves_its_target_alone() {
        // Given the user's directory replaced by a link to somewhere with a mode of its own
        let workspace = a_workspace();
        let socket = a_host_socket(workspace.path());
        let elsewhere = workspace.path().join("elsewhere");
        std::fs::create_dir(&elsewhere).unwrap();
        std::fs::set_permissions(&elsewhere, std::fs::Permissions::from_mode(0o755)).unwrap();
        let directory = socket.path.parent().unwrap();
        std::fs::create_dir_all(directory.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&elsewhere, directory).unwrap();

        // When
        let error = bind_host_socket("tddy-daemon", &socket).expect_err("refused");

        // Then — nothing was chmod'ed or chown'ed through the link, and nothing was bound in it
        assert!(!format!("{error:#}").is_empty());
        assert_eq!(mode_of(&elsewhere), 0o755);
        assert!(std::fs::read_dir(&elsewhere).unwrap().next().is_none());
    }

    #[test]
    fn refuses_a_path_somebody_else_could_redirect() {
        // Given a directory above the socket's that anybody may write to and that is not sticky
        let workspace = a_workspace();
        let run: PathBuf = workspace.path().join("run");
        std::fs::create_dir(&run).unwrap();
        std::fs::set_permissions(&run, std::fs::Permissions::from_mode(0o777)).unwrap();
        let socket = a_host_socket(workspace.path());

        // When
        let error = bind_host_socket("tddy-daemon", &socket).expect_err("refused");

        // Then — it could have been swapped for a link aimed at any file the supervisor can chown
        assert!(format!("{error:#}").contains("writable"), "{error:#}");
        assert!(!socket.path.exists());
    }
}
