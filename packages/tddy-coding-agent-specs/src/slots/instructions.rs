//! The `instructions` slot: a preamble prepended to the agent's context files, and the
//! append-system-prompt flag.

use std::path::{Path, PathBuf};

use super::SlotError;
use crate::InstructionsSlot;

/// Each of `slot.files` that exists under `workdir`, with `preamble` prepended — once: a file that
/// already leads with it is returned unchanged, so a re-run is idempotent.
pub fn render_instruction_files(
    slot: &InstructionsSlot,
    workdir: &Path,
    preamble: &str,
) -> Result<Vec<(PathBuf, Vec<u8>)>, SlotError> {
    let mut files = Vec::new();
    for relative in &slot.files {
        let path = workdir.join(relative);
        let body = match std::fs::read_to_string(&path) {
            Ok(body) => body,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                return Err(SlotError::Io {
                    path,
                    message: e.to_string(),
                })
            }
        };
        files.push((path, prepend_preamble(preamble, &body).into_bytes()));
    }
    Ok(files)
}

/// `body` led by `preamble`, or unchanged when it already is — the composition
/// `tddy_sandbox::prepend_preamble` performs for the jailed launch.
fn prepend_preamble(preamble: &str, body: &str) -> String {
    let preamble = preamble.trim();
    if body.trim_start().starts_with(preamble) {
        return body.to_string();
    }
    format!("{preamble}\n\n{body}")
}
