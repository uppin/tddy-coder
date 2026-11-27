//! A split session's pointer, placed with #829's machinery ([`crate::cli_session_pointer`]): the
//! host's [`SplitSessionPointers`] port (`#agent-tools 25/28`).
//!
//! A split agent runs on this host, in its context dir, so its pointer is always on this host: the
//! bounded ssh path of an ssh session's pointer is never taken here.

use std::path::Path;

use crate::cli_session_pointer::{
    place_session_pointer_locally, remove_pointer_file, removing_session_pointer_on_exit,
    session_pointer_path, PlacedSessionPointer, PointerHost,
};
use tddy_cli_sessions::cli_session_manager::ExitHook;
use tddy_core::session_pointer::SessionPointer;
use tddy_rpc::Status;
use tddy_session_split::split_ports::SplitSessionPointers;

/// [`SplitSessionPointers`] on this host: [`crate::cli_session_pointer::place_session_pointer_locally`]
/// and [`crate::cli_session_pointer::removing_session_pointer_on_exit`].
pub struct HostSplitSessionPointers;

impl SplitSessionPointers for HostSplitSessionPointers {
    fn place(
        &self,
        pointer: &SessionPointer,
        context_dir: &Path,
        on_exit: Option<ExitHook>,
    ) -> Result<Option<ExitHook>, Status> {
        let path = place_session_pointer_locally(&tddy_fs::DiskFs, pointer, context_dir)?;
        let placed = PlacedSessionPointer {
            path,
            host: PointerHost::Local,
        };
        Ok(removing_session_pointer_on_exit(
            std::sync::Arc::new(tddy_fs::DiskFs),
            on_exit,
            Some(placed),
            false,
        ))
    }

    fn remove(&self, context_dir: &Path) {
        remove_pointer_file(&tddy_fs::DiskFs, &session_pointer_path(context_dir));
    }
}

#[cfg(test)]
mod split_session_pointers_tests;
