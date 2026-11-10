//! The flag slots: `tool_allow`, `tool_deny` and `permission_prompt`.

use crate::spec::slot_shapes::FlagSlot;

/// `slot.flag` before each of `values`, in order: `[--allowedTools, a, --allowedTools, b]` — one
/// flag per entry, as the hand-built launch passed them.
pub fn render_flag_per_value(slot: &FlagSlot, values: &[String]) -> Vec<String> {
    values
        .iter()
        .flat_map(|value| [slot.flag.clone(), value.clone()])
        .collect()
}
