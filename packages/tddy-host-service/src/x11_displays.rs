//! The X11 displays on this host that the daemon's own OS user can open.
//!
//! Each running X server leaves a Unix socket named `X<N>` in its socket directory (by default
//! `/tmp/.X11-unix`), where `N` is its display number. A display is offered as a desktop channel
//! only when a connect to that socket succeeds as the daemon's user: a socket file outlives a
//! crashed server, and one owned by another user's server is not this daemon's to serve.
//!
//! **Connect and close — no X protocol is spoken.** Like the endpoint probe beside it
//! ([`crate::remote_desktop_probe`]), the connect is the whole question.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_daemon_kernel::config::ScreenSharingConfig;

use crate::remote_desktop_probe::{DesktopReachability, RemoteDesktopProbe};

/// The socket directory used when `screen_sharing.x11_socket_dir` is not configured.
pub const DEFAULT_X11_SOCKET_DIR: &str = "/tmp/.X11-unix";

/// An X11 display whose socket accepted a connection from this daemon's user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct X11Display {
    /// The display number: socket `X1` is display `:1`.
    pub number: u32,
    /// The socket the display was found on.
    pub socket: PathBuf,
}

/// List the displays in `socket_dir` whose `X<N>` socket accepts a connection, ordered by number.
///
/// A directory that does not exist is a host with no X server, and lists nothing. Entries whose
/// name is not `X` followed by a number, and sockets nothing answers on, are skipped.
pub fn list_x11_displays(socket_dir: &Path) -> std::io::Result<Vec<X11Display>> {
    // TODO(x11-channels): implement
    let _ = socket_dir;
    todo!()
}

/// The desktop block's readings with one X11 channel appended per display listed in
/// `screen_sharing.x11_socket_dir`.
///
/// A display already served on its VNC port (`5900 + N`, checked through `probe`) is left out, so a
/// desktop is never offered twice — once as an endpoint and once as a display. `can_bridge` on an
/// X11 channel requires `x11vnc` (`resolve_x11vnc_binary_path`) as well as the VNC bridge.
pub(crate) fn with_x11_channels(
    endpoints: Vec<DesktopReachability>,
    screen_sharing: &ScreenSharingConfig,
    probe: &Arc<dyn RemoteDesktopProbe>,
) -> Vec<DesktopReachability> {
    // TODO(x11-channels): implement
    let _ = (screen_sharing, probe);
    let _ = endpoints;
    todo!()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::test_util::{an_x11_socket_dir, AnX11SocketDir};

    fn display(number: u32, on: &AnX11SocketDir) -> X11Display {
        X11Display {
            number,
            socket: on.socket_of(number),
        }
    }

    #[test]
    fn lists_each_display_the_user_can_connect_to() {
        // Given
        let sockets = an_x11_socket_dir()
            .with_a_running_display(1)
            .with_a_running_display(0);

        // When
        let displays = list_x11_displays(sockets.path()).expect("a readable socket directory");

        // Then
        assert_eq!(displays, vec![display(0, &sockets), display(1, &sockets)]);
    }

    #[test]
    fn skips_a_display_socket_nothing_answers_on() {
        // Given
        let sockets = an_x11_socket_dir()
            .with_a_running_display(0)
            .with_a_dead_display(2);

        // When
        let displays = list_x11_displays(sockets.path()).expect("a readable socket directory");

        // Then
        assert_eq!(displays, vec![display(0, &sockets)]);
    }

    #[test]
    fn a_host_with_no_x11_socket_directory_lists_no_displays() {
        // Given
        let sockets = an_x11_socket_dir();
        let missing = sockets.path().join("no-x-server-here");

        // When
        let displays = list_x11_displays(&missing).expect("a missing directory is not an error");

        // Then
        assert_eq!(displays, Vec::<X11Display>::new());
    }
}
