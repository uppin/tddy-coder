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

use crate::host_tooling::ProbeOutcome;
use crate::remote_desktop_probe::{
    binary_is_present, DesktopChannel, DesktopProtocol, DesktopReachability, RemoteDesktopProbe,
    DEFAULT_VNC_PORT,
};

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
    let entries = match std::fs::read_dir(socket_dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };

    let mut displays = Vec::new();
    for entry in entries {
        let socket = entry?.path();
        let Some(number) = socket
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(display_number_of)
        else {
            continue;
        };
        if accepts_connections(&socket) {
            displays.push(X11Display { number, socket });
        }
    }
    displays.sort_by_key(|display| display.number);
    Ok(displays)
}

/// `1` out of `X1`; nothing for any other name.
fn display_number_of(file_name: &str) -> Option<u32> {
    let digits = file_name.strip_prefix('X')?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// Whether a connect to `socket` succeeds as this process's user — refused for a dead server,
/// denied for another user's.
#[cfg(unix)]
fn accepts_connections(socket: &Path) -> bool {
    std::os::unix::net::UnixStream::connect(socket).is_ok()
}

/// There are no X server sockets to connect to off Unix.
#[cfg(not(unix))]
fn accepts_connections(_socket: &Path) -> bool {
    false
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
    let socket_dir = socket_dir_of(screen_sharing);
    let displays = match list_x11_displays(Path::new(socket_dir)) {
        Ok(displays) => displays,
        Err(e) => {
            log::warn!("could not list X11 displays in {socket_dir}: {e}");
            return endpoints;
        }
    };

    let x11vnc_present = binary_is_present(
        &tddy_daemon_kernel::config::resolve_x11vnc_binary_path(screen_sharing),
    );
    let mut readings = endpoints;
    readings.extend(
        displays
            .iter()
            .filter_map(|display| x11_channel_reading(display, probe, x11vnc_present)),
    );
    readings
}

/// `screen_sharing.x11_socket_dir`, or [`DEFAULT_X11_SOCKET_DIR`] when it is not configured.
fn socket_dir_of(screen_sharing: &ScreenSharingConfig) -> &str {
    if screen_sharing.x11_socket_dir.is_empty() {
        DEFAULT_X11_SOCKET_DIR
    } else {
        screen_sharing.x11_socket_dir.as_str()
    }
}

/// The X11 channel reading for `display`, or `None` when the display is already served on its VNC
/// port (`5900 + N`) and so is offered as that endpoint instead.
fn x11_channel_reading(
    display: &X11Display,
    probe: &Arc<dyn RemoteDesktopProbe>,
    x11vnc_present: bool,
) -> Option<DesktopReachability> {
    let served_on_vnc_port = u16::try_from(u32::from(DEFAULT_VNC_PORT) + display.number)
        .ok()
        .map(|port| probe.probe(DesktopProtocol::Vnc, port));
    if served_on_vnc_port
        .as_ref()
        .is_some_and(|reading| reading.desktop_reachable)
    {
        return None;
    }
    // The bridge half of `can_bridge` is the VNC bridge's, which any endpoint reading of the
    // same probe carries; asking the probe about the display's own port yields it as well.
    let vnc_bridge_present = served_on_vnc_port
        .map(|reading| reading.can_bridge)
        .unwrap_or_else(|| {
            probe
                .probe(DesktopProtocol::Vnc, DEFAULT_VNC_PORT)
                .can_bridge
        });
    Some(DesktopReachability {
        outcome: ProbeOutcome::Ok,
        channel: DesktopChannel::X11Display(display.number),
        protocol: DesktopProtocol::Vnc,
        can_bridge: x11vnc_present && vnc_bridge_present,
        desktop_reachable: true,
        // No port until x11vnc is started for the display.
        port: 0,
    })
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
    fn ignores_entries_that_are_not_display_sockets() {
        // Given
        let sockets = an_x11_socket_dir()
            .with_a_running_display(0)
            .with_a_listening_socket_named("X1-lock")
            .with_a_listening_socket_named("X")
            .with_a_listening_socket_named("Xabc");

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
