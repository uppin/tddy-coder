//! Bridge inspect mode: which sign-in methods a desktop advertises, read without signing in.
//!
//! The daemon runs a bridge binary with `--inspect` and writes an [`InspectTarget`] to its stdin.
//! The bridge reads the desktop's advertisement — the RFB security-type list for VNC, the X.224
//! Connection Confirm for RDP — disconnects, and writes one [`InspectReport`] line to its stdout.
//!
//! Runs only when an operator opens a desktop. The periodic probe stays a bare TCP connect, and
//! inspect never sends a credential: it stops before any sign-in exchange begins.

use serde::{Deserialize, Serialize};

use crate::client::ScreenSharingClient;

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

/// What the daemon asked one run of a bridge binary to do, from its arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BridgeMode {
    /// No arguments: read a `BridgeConfig` from stdin and bridge the desktop into LiveKit.
    Bridge,
    /// `--inspect`: read an [`InspectTarget`] from stdin and report what the desktop advertises.
    Inspect,
}

impl BridgeMode {
    /// The mode `args` (the binary's arguments, without its own name) select.
    pub fn from_args(mut args: impl Iterator<Item = String>) -> anyhow::Result<Self> {
        match (args.next(), args.next()) {
            (None, _) => Ok(Self::Bridge),
            (Some(flag), None) if flag == INSPECT_FLAG => Ok(Self::Inspect),
            (Some(first), _) => anyhow::bail!(
                "unexpected argument '{first}': the only argument a bridge takes is {INSPECT_FLAG}"
            ),
        }
    }
}

/// The argument that puts a bridge binary in inspect mode.
pub const INSPECT_FLAG: &str = "--inspect";

/// Write `report` to `out` as one JSON line, and flush it.
pub fn write_inspect_report(
    out: &mut impl std::io::Write,
    report: &InspectReport,
) -> std::io::Result<()> {
    // TODO(sign-in-form): implement
    let _ = (out, report);
    todo!()
}

/// Parse one line a bridge wrote with [`write_inspect_report`].
pub fn read_inspect_report(line: &str) -> Result<InspectReport, serde_json::Error> {
    // TODO(sign-in-form): implement
    let _ = line;
    todo!()
}

/// Inspect mode's whole run: ask `C` what the desktop at `target` advertises, and write the
/// [`InspectReport`] line to stdout.
///
/// An inspection that fails — the desktop is unreachable, or broke the protocol — is returned as an
/// error, so the bridge exits non-zero with the reason on stderr and writes no report.
pub async fn run_inspect<C: ScreenSharingClient>(target: InspectTarget) -> anyhow::Result<()> {
    // TODO(sign-in-form): implement
    let _ = target;
    todo!()
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

    fn args(words: &[&str]) -> impl Iterator<Item = String> {
        words
            .iter()
            .map(|word| word.to_string())
            .collect::<Vec<_>>()
            .into_iter()
    }

    #[test]
    fn a_bridge_given_no_arguments_bridges() {
        assert_eq!(
            BridgeMode::from_args(args(&[])).expect("no arguments is a mode"),
            BridgeMode::Bridge
        );
    }

    #[test]
    fn a_bridge_given_the_inspect_flag_inspects() {
        assert_eq!(
            BridgeMode::from_args(args(&["--inspect"])).expect("--inspect is a mode"),
            BridgeMode::Inspect
        );
    }

    #[test]
    fn a_bridge_given_any_other_argument_refuses_it_by_name() {
        // When
        let refusal = BridgeMode::from_args(args(&["--password=hunter2-is-not-an-argument"]))
            .expect_err("an unknown argument is refused");

        // Then
        assert!(
            refusal
                .to_string()
                .contains("--password=hunter2-is-not-an-argument"),
            "the refusal does not name the argument: {refusal}"
        );
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
