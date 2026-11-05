//! The Cursor launch's notify environment (`chat`): what `tddy-tools notify` needs to reach a
//! session's operator. Every cursor-cli start, resume and re-attach is the generic CLI launch
//! (`opencode_cli_spawn::start_cli_harness_session`), which renders `cursor.yaml`.

pub use crate::chat;
pub use chat::*;
