//! The wasmtime component host an opted-in extension's `extension.wasm` runs in (R1, R3, R5, R6).
//!
//! The bindings are generated from Zed's WIT `since_v0.6.0`, vendored unmodified under
//! `wit/since_v0.6.0` (Apache-2.0, `wit/LICENSE-APACHE`; source commit in `wit/README.md`).
//!
//! The engine is this host's own, not the grammar store's: R5 turns on epoch interruption, and an
//! engine with it on makes every store on it trap at its first epoch check unless that store sets
//! a deadline — which tree-sitter's `WasmStore` never does. One wasmtime *version* serves both
//! (R6); one `Engine` cannot.

use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use wasmtime::component::{Component, HasSelf, Linker, Resource};
use wasmtime::{Config, Engine, Store, Trap};

use crate::index::IndexEntry;
use crate::manifest::LibEntry;
use crate::settings::ExtensionSettings;
use crate::wasm::imports::HostState;
use crate::wasm::wasi;
use crate::wasm::{
    supports_api_version, WasmCommandError, WasmWorktree, CALL_DEADLINE, EXTENSION_WASM,
};

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

/// How often the engine's epoch advances: a call's deadline is counted in these, so it is
/// enforced to within one tick.
const EPOCH_TICK: Duration = Duration::from_millis(10);

/// Advances an engine's epoch every [`EPOCH_TICK`] until dropped. A store's deadline is relative
/// to the epoch when it is set, so one ticker serves every store on the engine without one call's
/// deadline touching another's.
struct EpochTicker {
    /// Dropping it ends the ticker thread.
    _stop: mpsc::Sender<()>,
}

impl EpochTicker {
    fn start(engine: &Engine) -> Self {
        let (stop, stopped) = mpsc::channel::<()>();
        let engine = engine.clone();
        thread::spawn(move || {
            while stopped.recv_timeout(EPOCH_TICK) == Err(RecvTimeoutError::Timeout) {
                engine.increment_epoch();
            }
        });
        Self { _stop: stop }
    }
}

/// One engine and one linker with the host's imports, shared by every extension loaded through it.
pub struct WasmHost {
    engine: Engine,
    linker: Linker<HostState>,
    /// Kept alive by every extension loaded too: without it a call has no deadline.
    ticker: Arc<EpochTicker>,
    /// R5: how long each export call may run.
    deadline: Duration,
}

impl WasmHost {
    /// A host with the component model and epoch interruption on, every import of
    /// [`super::imports`] in its linker, and a [`CALL_DEADLINE`] deadline.
    pub fn new() -> wasmtime::Result<Self> {
        let mut config = Config::new();
        config.wasm_component_model(true).epoch_interruption(true);
        let engine = Engine::new(&config)?;
        let mut linker = Linker::new(&engine);
        bindings::Extension::add_to_linker::<HostState, HasSelf<HostState>>(
            &mut linker,
            |state| state,
        )?;
        wasi::bindings::WasiSubset::add_to_linker::<HostState, HasSelf<HostState>>(
            &mut linker,
            |state| state,
        )?;
        let ticker = Arc::new(EpochTicker::start(&engine));
        Ok(Self {
            engine,
            linker,
            ticker,
            deadline: CALL_DEADLINE,
        })
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
        if !lib.version.as_deref().is_some_and(supports_api_version) {
            return Err(WasmCommandError::UnsupportedApiVersion {
                extension: extension.to_string(),
                version: lib.version.clone(),
            });
        }
        let mut loaded = self.instantiate(extension, extension_dir, settings)?;
        loaded.init()?;
        Ok(loaded)
    }

    /// `extension.wasm` under `extension_dir` compiled and instantiated against this host's linker,
    /// in a store of its own; its `init-extension` not yet called.
    fn instantiate(
        &self,
        extension: &str,
        extension_dir: &Path,
        settings: &ExtensionSettings,
    ) -> Result<WasmExtension, WasmCommandError> {
        let load_error = |reason: String| load_error(extension, reason);
        let file = extension_dir.join(EXTENSION_WASM);
        let bytes =
            fs::read(&file).map_err(|error| load_error(format!("{}: {error}", file.display())))?;
        let component = Component::from_binary(&self.engine, &bytes)
            .map_err(|error| load_error(format!("{error:#}")))?;
        let mut store = Store::new(&self.engine, HostState::new(extension, settings.clone()));
        let instance = bindings::Extension::instantiate(&mut store, &component, &self.linker)
            .map_err(|error| load_error(format!("{error:#}")))?;
        Ok(WasmExtension {
            extension: extension.to_string(),
            store,
            instance,
            deadline: self.deadline,
            _ticker: Arc::clone(&self.ticker),
        })
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
        if !entry.wasm_enabled {
            return Ok(None);
        }
        let file = extension_dir.join(EXTENSION_WASM);
        match fs::metadata(&file) {
            Ok(_) => self.load(&entry.id, extension_dir, lib, settings).map(Some),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(error) => Err(load_error(
                &entry.id,
                format!("{}: {error}", file.display()),
            )),
        }
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
    store: Store<HostState>,
    instance: bindings::Extension,
    deadline: Duration,
    /// Keeps the epoch advancing, so [`Self::deadline`] is enforced.
    _ticker: Arc<EpochTicker>,
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
        let command = self.call(
            server,
            "language-server-command",
            worktree,
            |instance, store, handle| instance.call_language_server_command(store, server, handle),
        )?;
        Ok(WasmCommand {
            command: command.command,
            args: command.args,
            env: command.env,
        })
    }

    /// `language-server-initialization-options`, its JSON parsed; null when the extension has
    /// none.
    pub fn language_server_initialization_options(
        &mut self,
        server: &str,
        worktree: &WasmWorktree,
    ) -> Result<serde_json::Value, WasmCommandError> {
        let call = "language-server-initialization-options";
        let json = self.call(server, call, worktree, |instance, store, handle| {
            instance.call_language_server_initialization_options(store, server, handle)
        })?;
        self.parsed(server, call, json)
    }

    /// `language-server-workspace-configuration`, its JSON parsed; null when the extension has
    /// none.
    pub fn language_server_workspace_configuration(
        &mut self,
        server: &str,
        worktree: &WasmWorktree,
    ) -> Result<serde_json::Value, WasmCommandError> {
        let call = "language-server-workspace-configuration";
        let json = self.call(server, call, worktree, |instance, store, handle| {
            instance.call_language_server_workspace_configuration(store, server, handle)
        })?;
        self.parsed(server, call, json)
    }

    /// `init-extension`, under the deadline: what [`WasmHost::load`] calls once the instance exists.
    fn init(&mut self) -> Result<(), WasmCommandError> {
        self.arm_deadline();
        let deadline = self.deadline;
        self.instance
            .call_init_extension(&mut self.store)
            .map_err(|error| {
                load_error(
                    &self.extension,
                    match error.downcast_ref::<Trap>() {
                        Some(Trap::Interrupt) => format!(
                            "init-extension did not return within {deadline:?} and was stopped"
                        ),
                        _ => format!("init-extension failed: {error:#}"),
                    },
                )
            })
    }

    /// Epoch deadline for the call about to start: trap once [`Self::deadline`] has passed.
    fn arm_deadline(&mut self) {
        let ticks = self.deadline.as_millis().div_ceil(EPOCH_TICK.as_millis()) + 1;
        self.store
            .set_epoch_deadline(u64::try_from(ticks).unwrap_or(u64::MAX));
        self.store.epoch_deadline_trap();
    }

    /// One export call about `server`, with `worktree` handed to it, under the deadline.
    fn call<T>(
        &mut self,
        server: &str,
        call: &str,
        worktree: &WasmWorktree,
        export: impl FnOnce(
            &bindings::Extension,
            &mut Store<HostState>,
            Resource<WasmWorktree>,
        ) -> wasmtime::Result<Result<T, String>>,
    ) -> Result<T, WasmCommandError> {
        let handle = self
            .store
            .data_mut()
            .table
            .push(worktree.clone())
            .map_err(|error| self.trapped(server, call, error.to_string()))?;
        let rep = handle.rep();
        self.store.data_mut().server = Some(server.to_string());
        self.arm_deadline();
        let result = export(&self.instance, &mut self.store, handle);
        let state = self.store.data_mut();
        state.server = None;
        let released = state
            .table
            .delete(Resource::<WasmWorktree>::new_own(rep))
            .map(|_| ())
            .map_err(|error| error.to_string());
        self.outcome(server, call, result, released)
    }

    /// What one export call about `server` came to: its value once the worktree handed to it was
    /// released, the extension's own `Err` as [`WasmCommandError::Refused`], or its trap — the
    /// deadline's as [`WasmCommandError::DeadlineExceeded`].
    fn outcome<T>(
        &self,
        server: &str,
        call: &str,
        result: wasmtime::Result<Result<T, String>>,
        released: Result<(), String>,
    ) -> Result<T, WasmCommandError> {
        match result {
            Ok(Ok(value)) => released
                .map(|()| value)
                .map_err(|reason| self.trapped(server, call, reason)),
            Ok(Err(message)) => Err(WasmCommandError::Refused {
                extension: self.extension.clone(),
                server: server.to_string(),
                message,
            }),
            Err(error) => Err(match error.downcast_ref::<Trap>() {
                Some(Trap::Interrupt) => WasmCommandError::DeadlineExceeded {
                    extension: self.extension.clone(),
                    server: server.to_string(),
                    call: call.to_string(),
                    deadline: self.deadline,
                },
                _ => self.trapped(server, call, format!("{error:#}")),
            }),
        }
    }

    fn trapped(&self, server: &str, call: &str, reason: String) -> WasmCommandError {
        WasmCommandError::Trapped {
            extension: self.extension.clone(),
            server: server.to_string(),
            call: call.to_string(),
            reason,
        }
    }

    /// `json`, an extension's optional JSON answer, parsed; null when it gave none.
    fn parsed(
        &self,
        server: &str,
        call: &str,
        json: Option<String>,
    ) -> Result<serde_json::Value, WasmCommandError> {
        match json {
            None => Ok(serde_json::Value::Null),
            Some(text) => serde_json::from_str(&text).map_err(|error| WasmCommandError::Refused {
                extension: self.extension.clone(),
                server: server.to_string(),
                message: format!("{call} returned text that is not JSON: {error}"),
            }),
        }
    }
}

/// [`WasmCommandError::Load`] for `extension`.
fn load_error(extension: &str, reason: String) -> WasmCommandError {
    WasmCommandError::Load {
        extension: extension.to_string(),
        reason,
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
