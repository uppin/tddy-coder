//! Bridge inspect mode: which sign-in methods a desktop advertises, read without signing in.
//!
//! The daemon runs a bridge binary with `--inspect` and writes an [`InspectTarget`] to its stdin.
//! The bridge reads the desktop's advertisement — the RFB security-type list for VNC, the X.224
//! Connection Confirm for RDP — disconnects, and writes one [`InspectReport`] line to its stdout.
//! The line's types live in `tddy_daemon_extensions::desktop_inspect`, re-exported here.
//!
//! Runs only when an operator opens a desktop. The periodic probe stays a bare TCP connect, and
//! inspect never sends a credential: it stops before any sign-in exchange begins.

use crate::client::ScreenSharingClient;

pub use tddy_daemon_extensions::desktop_inspect::{
    read_inspect_report, write_inspect_report, AdvertisedMethod, InspectReport, InspectTarget,
    INSPECT_FLAG,
};

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

/// Inspect mode's whole run: ask `C` what the desktop at `target` advertises, and write the
/// [`InspectReport`] line to stdout.
///
/// An inspection that fails — the desktop is unreachable, or broke the protocol — is returned as an
/// error, so the bridge exits non-zero with the reason on stderr and writes no report.
pub async fn run_inspect<C: ScreenSharingClient>(target: InspectTarget) -> anyhow::Result<()> {
    let methods = C::inspect(&target.host, target.port).await?;
    let mut stdout = std::io::stdout().lock();
    write_inspect_report(&mut stdout, &InspectReport { methods })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

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
}
