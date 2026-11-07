//! A split session's pointer, placed with #829's machinery ([`crate::cli_session_pointer`]): the
//! host's [`SplitSessionPointers`] port (`#agent-tools 25/28`).
//!
//! A split agent runs on this host, in its context dir, so its pointer is always on this host: the
//! bounded ssh path of an ssh session's pointer is never taken here.

use std::path::Path;

use tddy_cli_sessions::cli_session_manager::ExitHook;
use tddy_core::session_pointer::SessionPointer;
use tddy_rpc::Status;
use tddy_session_split::split_ports::SplitSessionPointers;

/// [`SplitSessionPointers`] on this host: [`crate::cli_session_pointer::place_session_pointer`]
/// and [`crate::cli_session_pointer::removing_session_pointer_on_exit`].
pub struct HostSplitSessionPointers;

impl SplitSessionPointers for HostSplitSessionPointers {
    fn place(
        &self,
        pointer: &SessionPointer,
        context_dir: &Path,
        on_exit: Option<ExitHook>,
    ) -> Result<Option<ExitHook>, Status> {
        // TODO(split-remote-env): implement — place `pointer` in `context_dir` on this host and
        // precede `on_exit` with its removal.
        let _ = (pointer, context_dir);
        Ok(on_exit)
    }
}

#[cfg(test)]
mod split_session_pointers_tests;
