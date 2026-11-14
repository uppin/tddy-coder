//! `coding_agent.turn_input`: how a new turn reaches a running session of the agent
//! (`#orchestrators` 4/11), which `SendSessionMessage` and `ResumeSession`'s first message deliver
//! by.
//!
//! ```yaml
//! turn_input: { kind: terminal_paste, submit_key: Enter }
//! turn_input: { kind: acp_prompt }
//! ```
//!
//! The slot is the only place an agent's delivery is named: the daemon reads it, never the agent's
//! id. An agent without the slot takes no message, and a session of it is refused one naming the
//! agent.

use serde::{Deserialize, Serialize};

use crate::terminal_keys::is_key_name;

/// The `turn_input` slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TurnInputSlot {
    /// Paste the message into the session's main terminal — as a bracketed paste when the terminal
    /// has enabled it — then press `submit_key`, a key name as `SendSessionKeys` takes it
    /// (`Enter`).
    TerminalPaste { submit_key: String },
    /// Send the message as a prompt over the session's ACP connection.
    AcpPrompt,
}

/// Why `slot` cannot be delivered by: a terminal paste whose submit key is not a key name.
pub(crate) fn validate_turn_input_slot(slot: &TurnInputSlot) -> Result<(), String> {
    match slot {
        TurnInputSlot::TerminalPaste { submit_key } if !is_key_name(submit_key) => Err(format!(
            "submit_key {submit_key:?} is not a key name: key names are case-sensitive, as in Enter, C-c or F1"
        )),
        _ => Ok(()),
    }
}
