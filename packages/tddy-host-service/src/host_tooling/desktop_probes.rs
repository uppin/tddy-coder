use crate::host_tooling::ProbeOutcome;
use crate::host_tooling::PROBED_PROTOCOLS;
use crate::host_tooling::PROBE_TIMEOUT;
use crate::remote_desktop_probe::{DesktopProtocol, DesktopReachability, RemoteDesktopProbe};
use std::sync::Arc;

/// Start one connect per protocol, without waiting for any of them.
///
/// Each reading is produced on a thread of its own, like every other probe in this module and for
/// the same reason: a connect that gets no answer costs
/// [`crate::remote_desktop_probe::CONNECT_TIMEOUT`], and a host serving neither protocol would cost
/// two of those in a row if they were checked one after the other.
pub(super) fn start_desktop_probes(
    probe: &Arc<dyn RemoteDesktopProbe>,
) -> Vec<(
    DesktopProtocol,
    std::sync::mpsc::Receiver<DesktopReachability>,
)> {
    PROBED_PROTOCOLS
        .iter()
        .map(|&protocol| {
            let probe = Arc::clone(probe);
            let (tx, reading) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(probe.probe(protocol, protocol.default_port()));
            });
            (protocol, reading)
        })
        .collect()
}

/// Wait for each connect until `deadline`, then give up on it.
///
/// A connect bounds itself well inside [`PROBE_TIMEOUT`], so reaching this deadline means the
/// reading never came back at all — a failed probe, and never the "nothing is serving here"
/// finding an operator would go and act on.
pub(super) fn desktop_readings_of(
    started: Vec<(
        DesktopProtocol,
        std::sync::mpsc::Receiver<DesktopReachability>,
    )>,
    deadline: std::time::Instant,
) -> Vec<DesktopReachability> {
    started
        .into_iter()
        .map(|(protocol, reading)| {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            match reading.recv_timeout(remaining) {
                Ok(reading) => reading,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => desktop_probe_failed(
                    protocol,
                    format!(
                        "the {protocol:?} desktop probe did not finish within {}s",
                        PROBE_TIMEOUT.as_secs()
                    ),
                ),
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => desktop_probe_failed(
                    protocol,
                    format!("the {protocol:?} desktop probe ended without reporting"),
                ),
            }
        })
        .collect()
}

/// A reading for a protocol nothing reported on.
///
/// The two flags carry the same neutral values `TcpRemoteDesktopProbe` gives a connect it could not
/// complete, and [`ProbeOutcome::Failed`] beside them is what says they are not findings. A reader
/// keying off `desktop_reachable` alone would report "no desktop" for a host nobody reached, which
/// is the whole reason the outcome travels with them.
fn desktop_probe_failed(protocol: DesktopProtocol, reason: String) -> DesktopReachability {
    DesktopReachability {
        outcome: ProbeOutcome::Failed(reason),
        channel: crate::remote_desktop_probe::DesktopChannel::Endpoint,
        protocol,
        can_bridge: false,
        desktop_reachable: false,
        port: protocol.default_port(),
    }
}

/// Record a desktop probe that could not answer.
///
/// The same argument `warn_if_failed` makes for the three blocks beside this one: a `Failed`
/// outcome's cause lives on this side of the wire, and without a line in the daemon log its only
/// trace is a cell on a screen nobody may be looking at. No OS user is named, because a desktop is
/// served by the host rather than by one of its accounts.
pub(super) fn warn_if_a_desktop_probe_failed(readings: &[DesktopReachability]) {
    for reading in readings {
        if let ProbeOutcome::Failed(reason) = &reading.outcome {
            log::warn!(
                target: "tddy_daemon::host_tooling",
                "the {:?} desktop probe on port {} reported nothing: {reason}",
                reading.protocol,
                reading.port
            );
        }
    }
}
