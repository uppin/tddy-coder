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
    // TODO(claude-host): implement — the composition `tddy_sandbox::prepend_preamble` does today.
    let _ = (slot, workdir, preamble);
    todo!("claude-host: render_instruction_files")
}
