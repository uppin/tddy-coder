//! The wasmtime component host an opted-in extension's `extension.wasm` runs in (R1, R3, R5, R6).
//!
//! The bindings are generated from Zed's WIT `since_v0.6.0`, vendored unmodified under
//! `wit/since_v0.6.0` (Apache-2.0, `wit/LICENSE-APACHE`; source commit in `wit/README.md`).
//!
//! The engine is this host's own, not the grammar store's: R5 turns on epoch interruption, and an
//! engine with it on makes every store on it trap at its first epoch check unless that store sets
//! a deadline — which tree-sitter's `WasmStore` never does. One wasmtime *version* serves both
//! (R6); one `Engine` cannot.

use std::path::Path;
use std::time::Duration;

use wasmtime::component::Linker;
use wasmtime::{Engine, Store};

use crate::index::IndexEntry;
use crate::manifest::LibEntry;
use crate::settings::ExtensionSettings;
use crate::wasm::imports::HostState;
use crate::wasm::{WasmCommandError, WasmWorktree, CALL_DEADLINE};

/// The generated `zed:extension` world, version 0.6.x.
pub(crate) mod bindings {
    wasmtime::component::bindgen!({
        path: "wit/since_v0.6.0",
        world: "extension",
        imports: { default: trappable },
        with: {
            "worktree": crate::wasm::WasmWorktree,
        },
    });
}

/// One engine and one linker with the host's imports, shared by every extension loaded through it.
pub struct WasmHost {
    #[allow(
        dead_code,
        reason = "TODO(reshape-extension-wasm-command): read by load once implemented"
    )]
    engine: Engine,
    #[allow(
        dead_code,
        reason = "TODO(reshape-extension-wasm-command): read by load once implemented"
    )]
    linker: Linker<HostState>,
    /// R5: how long each export call may run.
    deadline: Duration,
}

impl WasmHost {
    /// A host with the component model and epoch interruption on, every import of
    /// [`super::imports`] in its linker, and a [`CALL_DEADLINE`] deadline.
    pub fn new() -> wasmtime::Result<Self> {
        // TODO(reshape-extension-wasm-command): implement
        todo!("WasmHost::new")
    }

    /// This host, stopping each call after `deadline` instead of [`CALL_DEADLINE`].
    pub fn with_deadline(self, deadline: Duration) -> Self {
        Self { deadline, ..self }
    }

    /// How long each export call may run.
    pub fn deadline(&self) -> Duration {
        self.deadline
    }

    /// The extension `extension` installed at `extension_dir`: R3 against `lib.version` first —
    /// before the file is read — then `extension.wasm` compiled, instantiated and its
    /// `init-extension` called. `settings` answer its `get-settings` calls.
    pub fn load(
        &self,
        extension: &str,
        extension_dir: &Path,
        lib: &LibEntry,
        settings: &ExtensionSettings,
    ) -> Result<WasmExtension, WasmCommandError> {
        let _ = (extension, extension_dir, lib, settings, CALL_DEADLINE);
        // TODO(reshape-extension-wasm-command): implement
        todo!("WasmHost::load")
    }

    /// R1: [`WasmHost::load`] for an extension whose index entry is opted in and whose
    /// `extension_dir` ships `extension.wasm`; `None` — nothing read — otherwise.
    pub fn load_opted_in(
        &self,
        entry: &IndexEntry,
        extension_dir: &Path,
        lib: &LibEntry,
        settings: &ExtensionSettings,
    ) -> Result<Option<WasmExtension>, WasmCommandError> {
        let _ = (entry, extension_dir, lib, settings);
        // TODO(reshape-extension-wasm-command): implement
        todo!("WasmHost::load_opted_in")
    }
}

/// The command an extension's `language-server-command` returned: Zed's `process.command`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmCommand {
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// One loaded extension: its store and its instance. Calls are serial — the store is `&mut`.
pub struct WasmExtension {
    extension: String,
    #[allow(
        dead_code,
        reason = "TODO(reshape-extension-wasm-command): read by the export calls once implemented"
    )]
    store: Store<HostState>,
    #[allow(
        dead_code,
        reason = "TODO(reshape-extension-wasm-command): read by the export calls once implemented"
    )]
    instance: bindings::Extension,
    #[allow(
        dead_code,
        reason = "TODO(reshape-extension-wasm-command): read by the export calls once implemented"
    )]
    deadline: Duration,
}

impl WasmExtension {
    /// The extension's id.
    pub fn extension(&self) -> &str {
        &self.extension
    }

    /// `language-server-command(server, worktree)`, within the deadline (R5). An `Err` the
    /// extension returns is [`WasmCommandError::Refused`] carrying its text — for a refusing import,
    /// [`super::download_refusal`] (R4).
    pub fn language_server_command(
        &mut self,
        server: &str,
        worktree: &WasmWorktree,
    ) -> Result<WasmCommand, WasmCommandError> {
        let _ = (server, worktree);
        // TODO(reshape-extension-wasm-command): implement
        todo!("WasmExtension::language_server_command")
    }

    /// `language-server-initialization-options`, its JSON parsed; null when the extension has
    /// none.
    pub fn language_server_initialization_options(
        &mut self,
        server: &str,
        worktree: &WasmWorktree,
    ) -> Result<serde_json::Value, WasmCommandError> {
        let _ = (server, worktree);
        // TODO(reshape-extension-wasm-command): implement
        todo!("WasmExtension::language_server_initialization_options")
    }

    /// `language-server-workspace-configuration`, its JSON parsed; null when the extension has
    /// none.
    pub fn language_server_workspace_configuration(
        &mut self,
        server: &str,
        worktree: &WasmWorktree,
    ) -> Result<serde_json::Value, WasmCommandError> {
        let _ = (server, worktree);
        // TODO(reshape-extension-wasm-command): implement
        todo!("WasmExtension::language_server_workspace_configuration")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn a_new_host_stops_a_call_after_ten_seconds() {
        // Given a host as production builds it
        let host = WasmHost::new().expect("a host");

        // When its deadline is read
        let deadline = host.deadline();

        // Then it is R5's ten seconds
        assert_eq!(deadline, Duration::from_secs(10));
    }
}
