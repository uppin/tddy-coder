//! The flag slots: `tool_allow`, `tool_deny` and `permission_prompt`.

use crate::FlagSlot;

/// `slot.flag` before each of `values`, in order: `[--allowedTools, a, --allowedTools, b]` — one
/// flag per entry, as the hand-built launch passed them.
pub fn render_flag_per_value(slot: &FlagSlot, values: &[String]) -> Vec<String> {
    // TODO(claude-host): implement.
    let _ = (slot, values);
    todo!("claude-host: render_flag_per_value")
}
