//! The bridge readiness protocol: one JSON line a bridge writes to its stdout once it either has a
//! picture to publish or knows it never will.
//!
//! The daemon spawns a bridge and, until this line arrives, knows nothing about it — not whether
//! the desktop accepted the sign-in, and not how large the desktop is. One line answers both, so
//! the daemon can fail a start with the desktop's own reason and report the real framebuffer size
//! the viewer scales clicks into.
//!
//! The line is the bridge's stdout and nothing else: the config already travels as JSON on stdin,
//! and stdout was unused, so the protocol needs no new channel between daemon and bridge.

use serde::{Deserialize, Serialize};

/// What a bridge reports once its start has settled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgeReadiness {
    /// Signed in, and the desktop's framebuffer is this size.
    Ready { width: u32, height: u32 },
    /// The start failed, and why.
    Failed { kind: FailureKind, message: String },
}

/// Which part of a bridge's start failed — the daemon maps each to a different status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    /// The desktop refused the sign-in: a wrong password, an unknown account.
    Authentication,
    /// The desktop could not be reached, or the protocol exchange failed for another reason.
    Connect,
    /// The bridge could not join the LiveKit room it was to publish into.
    LiveKit,
}

/// Write `readiness` to `out` as one JSON line, and flush it.
///
/// Flushed, because the daemon is blocked on this line: a report sitting in a buffer is a start
/// that times out while the desktop is already on screen.
pub fn write_readiness(
    out: &mut impl std::io::Write,
    readiness: &BridgeReadiness,
) -> std::io::Result<()> {
    // TODO(bridge-ready): implement
    let _ = (out, readiness);
    todo!()
}

/// Parse one line a bridge wrote with [`write_readiness`].
pub fn read_readiness(line: &str) -> Result<BridgeReadiness, serde_json::Error> {
    // TODO(bridge-ready): implement
    let _ = line;
    todo!()
}

/// The marker a bridge's `connect` error carries when the desktop refused the sign-in.
///
/// A type rather than a phrase, so the bridge classifies a failure by downcasting its error chain
/// instead of matching on whatever words a particular server chose to refuse with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticationRefused(pub String);

impl std::fmt::Display for AuthenticationRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "the desktop refused the sign-in: {}", self.0)
    }
}

impl std::error::Error for AuthenticationRefused {}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn the_line_written_for(readiness: &BridgeReadiness) -> String {
        let mut out = Vec::new();
        write_readiness(&mut out, readiness).expect("writing a readiness line to memory");
        String::from_utf8(out).expect("a readiness line is UTF-8")
    }

    #[test]
    fn a_ready_line_round_trips() {
        // Given
        let ready = BridgeReadiness::Ready {
            width: 1280,
            height: 800,
        };

        // When
        let line = the_line_written_for(&ready);

        // Then — exactly one line, and it reads back as what was written
        assert_eq!(line.matches('\n').count(), 1, "not one line: {line:?}");
        assert_eq!(
            read_readiness(line.trim_end()).expect("parsing the line just written"),
            ready
        );
    }

    #[test]
    fn a_failure_line_names_its_kind() {
        // Given — the line a bridge writes when the desktop refused its password
        let line = r#"{"failed":{"kind":"authentication","message":"the password is not right"}}"#;

        // When
        let readiness = read_readiness(line).expect("parsing a failure line");

        // Then
        assert_eq!(
            readiness,
            BridgeReadiness::Failed {
                kind: FailureKind::Authentication,
                message: "the password is not right".to_string(),
            }
        );
    }
}
