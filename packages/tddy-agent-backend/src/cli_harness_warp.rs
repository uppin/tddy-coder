//! Warp's Agent CLI (the `warp` binary) as a CLI harness: its descriptor, its argument list and the
//! reader of the resume token it prints on exit.
//!
//! `warp` takes no model and no first prompt on its command line — both are chosen inside the TUI
//! (`/model`, `/agent <prompt>`). A conversation is reopened with `warp --resume <TOKEN>`, and the
//! token exists only once a run ends: Warp prints the command to resume with as it exits. The daemon
//! reads it back off the terminal with [`resume_token_from_output`] and hands it to the next launch
//! as [`CliLaunch::conversation_id`].
//!
//! TODO(warp-cli-facts): the argv is verified against the real Warp Agent CLI (`warp --help`,
//! v0.2026.10.21); the exit banner's exact wording is still unobserved
//! (docs/dev/todo/2026-10-24-warp-cli-facts-unconfirmed.md).

// TODO(harness-clis): delete this module — the descriptor is `specs/warp.yaml`, and
// `resume_token_from_output` is the `warp-resume-token` adapter
// (`crate::adapters::WarpResumeToken`).

use crate::backend::BackendModels;
use crate::cli_harness::{CliHarness, CliLaunch, CliModelSource};
use crate::provider_route::Harness;

/// Warp offers no model to choose from outside its TUI.
#[must_use]
pub fn warp_cli_models() -> BackendModels {
    BackendModels {
        models: Vec::new(),
        default_model: String::new(),
    }
}

/// `warp` to start; `warp --resume <token>` to resume a conversation whose token was recorded. A
/// resume with no token starts a fresh conversation — never a bare `--resume`. The model and the
/// first prompt are not on Warp's command line, so neither is passed.
fn warp_argv(launch: &CliLaunch<'_>) -> Vec<String> {
    let mut argv = vec![launch.binary.to_string()];
    if launch.resume {
        if let Some(token) = launch
            .conversation_id
            .map(str::trim)
            .filter(|token| !token.is_empty())
        {
            argv.push("--resume".to_string());
            argv.push(token.to_string());
        }
    }
    argv
}

/// The token of the last `warp --resume <TOKEN>` line in `output`, a terminal's raw bytes with
/// their escape sequences; `None` when the run printed none.
#[must_use]
pub fn resume_token_from_output(output: &[u8]) -> Option<String> {
    let plain = String::from_utf8_lossy(&without_escape_sequences(output)).replace('\r', "\n");
    let mut token = None;
    for line in plain.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        for window in words.windows(3) {
            if let ["warp", "--resume", found] = window {
                token = Some(found.to_string());
            }
        }
    }
    token
}

const ESC: u8 = 0x1b;
const BEL: u8 = 0x07;

/// `output` without its escape sequences: CSI (`ESC [` … a final byte), the string sequences OSC,
/// DCS, SOS, PM and APC (closed by BEL or `ESC \`), and any other escape (its intermediate bytes,
/// then one final byte).
fn without_escape_sequences(output: &[u8]) -> Vec<u8> {
    let mut plain = Vec::with_capacity(output.len());
    let mut bytes = output.iter().copied();
    while let Some(byte) = bytes.next() {
        if byte != ESC {
            plain.push(byte);
            continue;
        }
        match bytes.next() {
            Some(b'[') => {
                bytes.find(|b| (0x40..=0x7e).contains(b));
            }
            Some(b']' | b'P' | b'X' | b'^' | b'_') => skip_string(&mut bytes),
            Some(0x20..=0x2f) => {
                bytes.find(|b| (0x30..=0x7e).contains(b));
            }
            Some(_) | None => {}
        }
    }
    plain
}

/// Consumes a string sequence up to and including its terminator: BEL, or `ESC \`.
fn skip_string(bytes: &mut impl Iterator<Item = u8>) {
    let mut after_esc = false;
    for byte in bytes {
        if byte == BEL || (after_esc && byte == b'\\') {
            return;
        }
        after_esc = byte == ESC;
    }
}

/// The Warp descriptor, registered in [`crate::cli_harness::cli_harnesses`].
pub const WARP_CLI: CliHarness = CliHarness {
    session_type: "warp-cli",
    label: "Warp",
    route_harness: Harness::WarpCli,
    context_agent: "warp",
    default_binary: "warp",
    binary_env: "TDDY_WARP_CLI",
    route_config_env: None,
    argv: warp_argv,
    models: CliModelSource::Curated(warp_cli_models),
};
