//! Whether a host's desktop can be reached — and whether this daemon could bridge it if it were.
//!
//! Two facts, kept apart on purpose:
//!
//! | Fact | Question it answers | How |
//! |---|---|---|
//! | **bridge capability** | can tddy stream a desktop from this host at all? | the resolved bridge binary exists |
//! | **desktop reachability** | is anything serving a desktop here? | a bounded TCP connect |
//!
//! Collapsing them would send an operator to the wrong fix: "install the bridge" and "start a VNC
//! server" are unrelated problems.
//!
//! The bridge check is the cheaper win of the two. `resolve_vnc_binary_path` /
//! `resolve_rdp_binary_path` (`crate::config`) resolve a path by *guessing* — explicit config, then
//! a sibling of `current_exe()`, then a bare name on `PATH` — with **no existence check anywhere**.
//! A missing binary surfaces only as a spawn error in `screen_sharing_service`, i.e. after the user
//! has already asked for a stream. Checking up front turns that into a fact on the row.

use std::io::ErrorKind;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::Path;
use std::time::Duration;

use crate::config::{resolve_rdp_binary_path, resolve_vnc_binary_path, DaemonConfig};
use crate::host_tooling::ProbeOutcome;

/// Default VNC port (display :0). Port discovery is out of scope — the probe reports which port it
/// checked so "unreachable" is not read as authoritative for a host serving elsewhere.
pub const DEFAULT_VNC_PORT: u16 = 5900;
/// Default RDP port.
pub const DEFAULT_RDP_PORT: u16 = 3389;

/// How long a connect may take before it is abandoned.
///
/// Short: the Hosts screen probes every host it lists, for both protocols, and an unreachable
/// address must not hold the tooling RPC open.
pub const CONNECT_TIMEOUT: Duration = Duration::from_millis(750);

/// Which remote-desktop protocol a reading is about.
///
/// The discriminants mirror `screen_sharing.proto`'s `Protocol` so the two cannot drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopProtocol {
    Vnc = 1,
    Rdp = 2,
}

impl DesktopProtocol {
    /// The port this probe checks when none is configured.
    #[must_use]
    pub fn default_port(self) -> u16 {
        match self {
            DesktopProtocol::Vnc => DEFAULT_VNC_PORT,
            DesktopProtocol::Rdp => DEFAULT_RDP_PORT,
        }
    }
}

/// What the probe found for one protocol on one host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopReachability {
    pub outcome: ProbeOutcome,
    pub protocol: DesktopProtocol,
    /// The bridge binary for this protocol exists and could be spawned.
    pub can_bridge: bool,
    /// Something accepted a connection on [`Self::port`].
    pub desktop_reachable: bool,
    pub port: u16,
}

/// Probes a host's remote-desktop reachability.
pub trait RemoteDesktopProbe: Send + Sync {
    /// Check `protocol` on `port`, bounded by [`CONNECT_TIMEOUT`].
    fn probe(&self, protocol: DesktopProtocol, port: u16) -> DesktopReachability;
}

/// Whether a TCP connect to `port` on loopback succeeds inside [`CONNECT_TIMEOUT`].
///
/// **Connect and close — no bytes are ever written.** A monitoring screen polling half-open protocol
/// handshakes against people's desktops on a timer is antisocial, and the connect already answers
/// the question being asked.
pub fn is_accepting_connections(host: &str, port: u16) -> Result<bool, String> {
    let address = (host, port)
        .to_socket_addrs()
        .map_err(|e| format!("could not resolve {host}:{port}: {e}"))?
        .next()
        .ok_or_else(|| format!("{host}:{port} resolved to no address"))?;

    match TcpStream::connect_timeout(&address, CONNECT_TIMEOUT) {
        // The whole interaction: the connection is established and immediately dropped. Nothing is
        // written, so no protocol handshake is ever begun against someone's desktop.
        Ok(_connected_and_closed) => Ok(true),
        // A refusal is an answer: we reached the host and nothing is listening there.
        Err(e) if e.kind() == ErrorKind::ConnectionRefused => Ok(false),
        // Everything else — a timeout, an unreachable network, a permission denial — means the
        // probe could not get an answer. Reporting that as "no desktop" would be a finding we never
        // made.
        Err(e) => Err(format!("could not check {host}:{port}: {e}")),
    }
}

/// The host a daemon probes: its own loopback. Each daemon reports for the machine it runs on, so a
/// desktop served on another host's loopback is that daemon's reading to take, not this one's.
const PROBE_HOST: &str = "127.0.0.1";

/// The live probe: an existence check on the resolved bridge binary plus a TCP connect.
pub struct TcpRemoteDesktopProbe;

impl RemoteDesktopProbe for TcpRemoteDesktopProbe {
    fn probe(&self, protocol: DesktopProtocol, port: u16) -> DesktopReachability {
        // Independent of the connect below, and answered even when the connect gets nowhere: a host
        // that cannot bridge cannot bridge whether or not a desktop is up.
        let can_bridge = bridge_binary_is_present(protocol);

        match is_accepting_connections(PROBE_HOST, port) {
            Ok(desktop_reachable) => DesktopReachability {
                outcome: ProbeOutcome::Ok,
                protocol,
                can_bridge,
                desktop_reachable,
                port,
            },
            Err(reason) => DesktopReachability {
                outcome: ProbeOutcome::Failed(reason),
                protocol,
                can_bridge,
                // Not a finding. `Failed` is what says so — a reader keying off this flag alone
                // would report "no desktop" for a host we never reached.
                desktop_reachable: false,
                port,
            },
        }
    }
}

/// Whether the bridge binary for `protocol` is actually there, at the path
/// [`crate::config`] would spawn it from.
///
/// TODO(desktop-probe): this reads the *default* configuration, so an operator's explicit
/// `screen_sharing.vnc_binary_path` / `rdp_binary_path` is not consulted. Threading the daemon's
/// live config in needs the probe to carry it, which its callers do not yet do.
fn bridge_binary_is_present(protocol: DesktopProtocol) -> bool {
    let config = DaemonConfig::default();
    let resolved = match protocol {
        DesktopProtocol::Vnc => resolve_vnc_binary_path(&config),
        DesktopProtocol::Rdp => resolve_rdp_binary_path(&config),
    };
    binary_is_present(Path::new(&resolved))
}

/// Whether a resolved binary path names something that exists.
///
/// A resolution with a directory in it is checked directly. A **bare name** — the resolvers' last
/// resort, which they hand to the OS to look up on `PATH` — is searched on `PATH` too: testing it
/// with `exists()` would answer about the daemon's working directory, which is not where the OS
/// would find it.
fn binary_is_present(resolved: &Path) -> bool {
    let has_directory = resolved
        .parent()
        .is_some_and(|parent| !parent.as_os_str().is_empty());
    if has_directory {
        return resolved.is_file();
    }

    let Some(path_var) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path_var)
        .filter(|dir| !dir.as_os_str().is_empty())
        .any(|dir| dir.join(resolved).is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// Bind an ephemeral loopback port so the test needs no fixed port and no network.
    fn a_listener() -> (TcpListener, u16) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let port = listener.local_addr().unwrap().port();
        (listener, port)
    }

    /// A port nothing is listening on: bind, read the port, then drop the listener.
    fn a_closed_port() -> u16 {
        let (listener, port) = a_listener();
        drop(listener);
        port
    }

    #[test]
    fn reports_a_port_with_a_listener_as_reachable() {
        let (_listener, port) = a_listener();

        let reachable = is_accepting_connections("127.0.0.1", port).expect("a bounded probe");

        assert!(reachable, "a bound port accepts connections");
    }

    /// A refused connect is a *finding*, not a failure — and it must be distinguishable from a
    /// timeout, which is what AC-5 turns on.
    #[test]
    fn reports_a_port_with_no_listener_as_unreachable() {
        let port = a_closed_port();

        let reachable =
            is_accepting_connections("127.0.0.1", port).expect("a refusal is not an error");

        assert!(!reachable, "nothing is listening there");
    }

    /// The rudeness guard. "We don't handshake" is a comment until something checks it: the listener
    /// records what it actually received, and the probe must have written nothing before closing.
    #[test]
    fn writes_no_bytes_to_the_remote_before_closing() {
        let (listener, port) = a_listener();
        let bytes_received = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&bytes_received);

        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                stream
                    .set_read_timeout(Some(Duration::from_millis(200)))
                    .ok();
                let mut buf = [0u8; 64];
                if let Ok(n) = stream.read(&mut buf) {
                    seen.fetch_add(n, Ordering::SeqCst);
                }
            }
        });

        let _ = is_accepting_connections("127.0.0.1", port);
        std::thread::sleep(Duration::from_millis(300));

        assert_eq!(
            bytes_received.load(Ordering::SeqCst),
            0,
            "the probe must connect and close, never begin a protocol handshake"
        );
    }

    /// Bridge capability and desktop reachability are independent: a host can have a serving desktop
    /// and no bridge binary, and the row must say which is missing.
    #[test]
    fn reports_that_the_host_cannot_bridge_when_the_binary_is_missing() {
        let (_listener, port) = a_listener();

        let reading = TcpRemoteDesktopProbe.probe(DesktopProtocol::Vnc, port);

        assert_eq!(reading.outcome, ProbeOutcome::Ok);
        assert_eq!(reading.port, port, "the checked port is reported back");
        assert!(
            reading.desktop_reachable,
            "something is listening on that port"
        );
        assert!(
            !reading.can_bridge,
            "no tddy-vnc binary is beside the test binary, so this host cannot bridge — a separate \
             fact from the desktop being reachable"
        );
    }

    /// Default ports are reported, not silently assumed, so "unreachable" is never read as
    /// authoritative for a host serving somewhere else.
    #[test]
    fn names_the_default_port_for_each_protocol() {
        assert_eq!(DesktopProtocol::Vnc.default_port(), 5900);
        assert_eq!(DesktopProtocol::Rdp.default_port(), 3389);
    }
}
