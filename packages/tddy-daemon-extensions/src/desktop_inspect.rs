//! The bridge inspect protocol: which sign-in methods a desktop advertises, read without signing in.
//!
//! The daemon runs a bridge binary with [`INSPECT_FLAG`] and writes an [`InspectTarget`] to its
//! stdin. The bridge reads the desktop's advertisement — the RFB security-type list for VNC, the
//! X.224 Connection Confirm for RDP — disconnects, and writes one [`InspectReport`] line to its
//! stdout.
//!
//! Like [`crate::bridge_readiness`], it lives in the extension ports so the daemon reads the line
//! without depending on the bridge crate, which is LiveKit's (`tests/livekit_is_a_satellite.rs`).
//! The bridge side — the argument parsing and the run itself — stays in `tddy-screenshare`.

use serde::{Deserialize, Serialize};

/// What the daemon writes to a bridge's stdin in inspect mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InspectTarget {
    pub host: String,
    pub port: u16,
}

/// One sign-in method a desktop advertises.
///
/// Mirrors `screen_sharing.proto`'s `DesktopAuthMethod`, plus the VNC security types tddy cannot
/// sign in with, kept by number so the operator can be told what the desktop wanted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdvertisedMethod {
    /// VNC security type 1: no authentication.
    None,
    /// VNC security type 2: VNC Authentication.
    VncPassword,
    /// VeNCrypt subtype Plain (256).
    VencryptPlain,
    /// VeNCrypt subtype TLSPlain (259).
    VencryptTlsPlain,
    /// VNC security type 30: Apple Remote Desktop.
    AppleRemoteDesktop,
    /// RDP over TLS only.
    RdpTls,
    /// RDP with Network Level Authentication (CredSSP).
    RdpNla,
    /// A VNC security type tddy cannot sign in with.
    UnsupportedVnc { security_type: u8 },
}

/// The one line a bridge writes to its stdout in inspect mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InspectReport {
    /// Every method the desktop advertised, in the order it advertised them.
    pub methods: Vec<AdvertisedMethod>,
}

/// The argument that puts a bridge binary in inspect mode.
pub const INSPECT_FLAG: &str = "--inspect";

/// Write `report` to `out` as one JSON line, and flush it.
pub fn write_inspect_report(
    out: &mut impl std::io::Write,
    report: &InspectReport,
) -> std::io::Result<()> {
    serde_json::to_writer(&mut *out, report)?;
    out.write_all(b"\n")?;
    out.flush()
}

/// Parse one line a bridge wrote with [`write_inspect_report`].
pub fn read_inspect_report(line: &str) -> Result<InspectReport, serde_json::Error> {
    serde_json::from_str(line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn the_line_written_for(report: &InspectReport) -> String {
        let mut out = Vec::new();
        write_inspect_report(&mut out, report).expect("writing an inspect report to memory");
        String::from_utf8(out).expect("an inspect report is UTF-8")
    }

    #[test]
    fn an_inspect_report_is_one_line_that_reads_back_as_written() {
        // Given
        let report = InspectReport {
            methods: vec![
                AdvertisedMethod::VncPassword,
                AdvertisedMethod::UnsupportedVnc { security_type: 16 },
            ],
        };

        // When
        let line = the_line_written_for(&report);

        // Then
        assert_eq!(line.matches('\n').count(), 1, "not one line: {line:?}");
        assert_eq!(
            read_inspect_report(line.trim_end()).expect("parsing the line just written"),
            report
        );
    }

    #[test]
    fn an_inspect_line_names_each_method_in_snake_case() {
        // Given — the line a VNC bridge writes for a desktop offering ARD next to type 16
        let line =
            r#"{"methods":["apple_remote_desktop",{"unsupported_vnc":{"security_type":16}}]}"#;

        // When
        let report = read_inspect_report(line).expect("parsing an inspect line");

        // Then
        assert_eq!(
            report.methods,
            vec![
                AdvertisedMethod::AppleRemoteDesktop,
                AdvertisedMethod::UnsupportedVnc { security_type: 16 },
            ]
        );
    }
}
