//! An opted-in extension's `extension.wasm` says how to start its language server (`#reshape`
//! extension-wasm-command, R1–R6).
//!
//! Zed takes a language server's command from the extension's WASM unless the user set
//! `lsp.<id>.binary.path`. tddy does the same for an extension the user opted in
//! (`tddy-tools extensions enable-wasm <id>`, recorded as [`crate::IndexEntry::wasm_enabled`]),
//! over a minimal host ([`host`], [`imports`]): the worktree, settings and platform are answered,
//! and every call that downloads or installs is refused with [`download_refusal`].
//!
//! - **R1** — no `.wasm` is loaded for an extension that is not opted in ([`WasmHost::load_opted_in`]).
//! - **R2** — a configured binary path wins; the WASM is asked only for a server K22 left
//!   unconfigured; with no WASM, K22's refusal stands ([`choose_server`]).
//! - **R3** — `lib.version` outside [`SUPPORTED_API_VERSIONS`] is refused with the range.
//! - **R4** — the refusing imports return [`download_refusal`], and a command that failed through
//!   one carries it ([`WasmCommandError::Refused`]).
//! - **R5** — each export call has a [`CALL_DEADLINE`] epoch deadline.
//! - **R6** — one wasmtime: the version tree-sitter's `wasm` feature pins (see `Cargo.toml`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::dir::ExtensionsDir;
use crate::error::ExtensionError;
use crate::index::{Index, IndexEntry};
use crate::language::ExtensionLanguage;
use crate::manifest::{ExtensionManifest, LibEntry};
use crate::server::{client_capabilities, Served, ServerChoice, ServerRefusal};
use crate::settings::ExtensionSettings;
use tddy_lsp::LaunchSpec;

pub mod host;
pub mod imports;
mod wasi;

pub use host::{WasmCommand, WasmExtension, WasmHost};

/// The file an extension with code ships beside its `extension.toml`.
pub const EXTENSION_WASM: &str = "extension.wasm";

/// The `zed_extension_api` versions tddy runs, as a refusal names them (PRD F1 a): Zed's WIT
/// `since_v0.6.0`, which covers 0.6.0 up to, not including, 0.7.0.
pub const SUPPORTED_API_VERSIONS: &str = "0.6.0..<0.7.0";

/// How long one export call may run before it is stopped (R5).
pub const CALL_DEADLINE: Duration = Duration::from_secs(10);

/// Whether `version` — an extension's `lib.version` — is in [`SUPPORTED_API_VERSIONS`]: a
/// `major.minor.patch` with major 0 and minor 6. Anything that does not parse is unsupported.
pub fn supports_api_version(version: &str) -> bool {
    let mut parts = version.split('.');
    let numbers: Option<Vec<u64>> = parts
        .by_ref()
        .take(3)
        .map(|part| part.parse().ok())
        .collect();
    matches!((numbers.as_deref(), parts.next()), (Some([0, 6, _]), None))
}

/// What every refusing import returns to the extension while it computes `server`'s command (R4),
/// and what the run's refusal then carries:
/// `tddy does not download language servers; install <server> and set lsp.<server>.binary.path`.
pub fn download_refusal(server: &str) -> String {
    format!(
        "tddy does not download language servers; install {server} and set lsp.{server}.binary.path"
    )
}

/// The worktree an export call is asked about — Zed's `worktree` resource, as tddy answers it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmWorktree {
    /// `worktree.id()`.
    pub id: u64,
    /// `worktree.root-path()`; `read-text-file` paths are relative to it and may not leave it.
    pub root: PathBuf,
    /// `worktree.shell-env()`, and the `PATH` `worktree.which` searches — never this process's
    /// own environment unless the caller put it here.
    pub shell_env: Vec<(String, String)>,
}

/// Why an extension's WASM gave no command.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WasmCommandError {
    /// R3: `lib.version` is outside [`SUPPORTED_API_VERSIONS`], or absent.
    #[error(
        "extension {extension}: built for zed_extension_api {}; tddy supports {SUPPORTED_API_VERSIONS}",
        version.as_deref().unwrap_or("<no lib.version>")
    )]
    UnsupportedApiVersion {
        extension: String,
        version: Option<String>,
    },

    /// `extension.wasm` is missing, is not a component, or does not instantiate against the host.
    #[error("extension {extension}: {EXTENSION_WASM} did not load: {reason}")]
    Load { extension: String, reason: String },

    /// The export returned an error — for a refusing import, [`download_refusal`]'s text (R4).
    #[error("extension {extension}: language server {server}: {message}")]
    Refused {
        extension: String,
        server: String,
        message: String,
    },

    /// R5: the call ran past the host's deadline and was stopped.
    #[error("extension {extension}: {call} for {server} did not return within {deadline:?} and was stopped")]
    DeadlineExceeded {
        extension: String,
        server: String,
        /// The export, as the WIT names it: `language-server-command`, …
        call: String,
        deadline: Duration,
    },

    /// The call trapped for any other reason.
    #[error("extension {extension}: {call} for {server} trapped: {reason}")]
    Trapped {
        extension: String,
        server: String,
        call: String,
        reason: String,
    },
}

/// R2 for `language`, an extension language `manifest` declares: what serves it once an opted-in
/// extension's WASM has been asked.
///
/// `choice` is K22's [`ServerChoice::for_language`] for the same language. A [`ServerChoice::Configured`] server keeps
/// its binary and the WASM is never called; [`ServerChoice::Undeclared`] stays undeclared; a
/// [`ServerChoice::Unconfigured`] server with `wasm` (the extension is opted in and ships code)
/// becomes [`ServerChoice::Configured`] from the WASM's `language-server-command`,
/// `-initialization-options` and `-workspace-configuration` — with
/// [`crate::server::client_capabilities`] and the manifest's `language_ids` entry for the language
/// as its `language_id`, as K22 gives a configured one — or that call's [`WasmCommandError`]; with
/// no `wasm` it stays K22's refusal.
pub fn choose_server(
    manifest: &ExtensionManifest,
    language: &ExtensionLanguage,
    choice: ServerChoice,
    wasm: Option<&mut WasmExtension>,
    worktree: &WasmWorktree,
) -> Result<ServerChoice, WasmCommandError> {
    let (ServerChoice::Unconfigured(no_command), Some(wasm)) = (&choice, wasm) else {
        return Ok(choice);
    };
    let server = no_command.server.clone();
    let command = wasm.language_server_command(&server, worktree)?;
    let initialization_options = wasm.language_server_initialization_options(&server, worktree)?;
    let workspace_configuration =
        wasm.language_server_workspace_configuration(&server, worktree)?;
    let language_id = manifest
        .language_servers
        .get(&server)
        .and_then(|entry| entry.language_ids.get(&language.name))
        .cloned()
        .unwrap_or_default();
    let mut launch = LaunchSpec::new(command.command)
        .with_capabilities(client_capabilities())
        .with_initialization_options(initialization_options)
        .with_workspace_configuration(workspace_configuration)
        .with_language_id(language_id);
    launch.args = command.args;
    launch.env = command.env;
    Ok(ServerChoice::Configured { server, launch })
}

/// The worktree every extension is asked about when the installed extensions load: the process's
/// working directory — the workspace the CLI was run in; the support interface hands a language no
/// root before its files are resolved — with this process's own environment as `shell-env`, the
/// CLI being run from the user's shell. Id 1: there is one worktree.
fn the_process_worktree() -> Result<WasmWorktree, ExtensionError> {
    let root = std::env::current_dir()
        .map_err(|error| ExtensionError::io(std::path::Path::new("."), error))?;
    Ok(WasmWorktree {
        id: 1,
        root,
        shell_env: std::env::vars().collect(),
    })
}

/// Each of `languages` — an installed extension's, beside its manifest — with what serves it: K22's
/// choice from the settings ([`crate::server::served`]), then the opted-in extension's WASM for a
/// server K22 left unconfigured ([`served_by_wasm`]).
pub(crate) fn served(
    dir: &ExtensionsDir,
    languages: Vec<(ExtensionManifest, ExtensionLanguage)>,
) -> Result<Vec<Served>, ExtensionError> {
    let manifests: Vec<ExtensionManifest> = languages
        .iter()
        .map(|(manifest, _)| manifest.clone())
        .collect();
    let served = crate::server::served(dir, languages)?;
    served_by_wasm(dir, manifests.into_iter().zip(served).collect())
}

/// [`choose_server`] for each of `languages` — K22's [`Served`] beside its manifest — whose server
/// K22 left [`ServerChoice::Unconfigured`] and whose extension is opted in and ships code; every
/// other language keeps its choice, and nothing is read or started for it (R1, R2). A WASM that
/// cannot be loaded or gives no command refuses the languages it would have served, in its own
/// words ([`ServerChoice::Refused`]).
///
/// The code-action kinds are kept as K22's step found them: they are the manifest entry's for the
/// server, and the WASM names only that server's command.
pub(crate) fn served_by_wasm(
    dir: &ExtensionsDir,
    languages: Vec<(ExtensionManifest, Served)>,
) -> Result<Vec<Served>, ExtensionError> {
    let any_unconfigured = languages
        .iter()
        .any(|(_, (_, choice, _))| matches!(choice, ServerChoice::Unconfigured(_)));
    if !any_unconfigured {
        return Ok(languages.into_iter().map(|(_, served)| served).collect());
    }
    let mut opted_in = OptedInWasm::load(dir)?;
    Ok(languages
        .into_iter()
        .map(|(manifest, (language, choice, kinds))| {
            let choice = opted_in.serve(&manifest, &language, choice);
            (language, choice, kinds)
        })
        .collect())
}

/// What [`served_by_wasm`] asks the opted-in extensions with: the index that says who is opted in,
/// the worktree every call is about, and the extensions loaded so far.
struct OptedInWasm {
    index: Index,
    worktree: WasmWorktree,
    loader: WasmLoader,
}

impl OptedInWasm {
    fn load(dir: &ExtensionsDir) -> Result<Self, ExtensionError> {
        Ok(Self {
            index: Index::load(dir)?,
            worktree: the_process_worktree()?,
            loader: WasmLoader {
                settings: ExtensionSettings::load(dir)?,
                host: None,
                loaded: BTreeMap::new(),
            },
        })
    }

    /// `choice` for `language` once its extension's WASM has been asked — only when the server is
    /// unconfigured and the extension opted in. A [`WasmCommandError`] refuses the server with its
    /// text.
    fn serve(
        &mut self,
        manifest: &ExtensionManifest,
        language: &ExtensionLanguage,
        choice: ServerChoice,
    ) -> ServerChoice {
        let ServerChoice::Unconfigured(no_command) = &choice else {
            return choice;
        };
        let Some(entry) = self
            .index
            .extensions
            .get(&manifest.id)
            .filter(|entry| entry.wasm_enabled)
        else {
            return choice;
        };
        let server = no_command.server.clone();
        self.loader
            .extension(entry, &language.extension_dir, &manifest.lib)
            .and_then(|wasm| choose_server(manifest, language, choice, wasm, &self.worktree))
            .unwrap_or_else(|error| {
                ServerChoice::Refused(ServerRefusal {
                    server,
                    reason: error.to_string(),
                })
            })
    }
}

/// Each opted-in extension loaded at most once, through one host started by the first that needs
/// it: no opted-in extension, no engine.
struct WasmLoader {
    /// K22's settings, which every loaded extension's `get-settings` answers from.
    settings: ExtensionSettings,
    host: Option<Result<WasmHost, String>>,
    loaded: BTreeMap<String, Result<Option<WasmExtension>, WasmCommandError>>,
}

impl WasmLoader {
    /// [`WasmHost::load_opted_in`] for `entry`'s extension on the first ask, the same outcome after.
    fn extension(
        &mut self,
        entry: &IndexEntry,
        extension_dir: &Path,
        lib: &LibEntry,
    ) -> Result<Option<&mut WasmExtension>, WasmCommandError> {
        let Self {
            settings,
            host,
            loaded,
        } = self;
        let outcome = loaded.entry(entry.id.clone()).or_insert_with(|| {
            match host.get_or_insert_with(|| WasmHost::new().map_err(|error| error.to_string())) {
                Ok(host) => host.load_opted_in(entry, extension_dir, lib, settings),
                Err(reason) => Err(WasmCommandError::Load {
                    extension: entry.id.clone(),
                    reason: format!("the WASM host did not start: {reason}"),
                }),
            }
        });
        match outcome {
            Ok(wasm) => Ok(wasm.as_mut()),
            Err(error) => Err(error.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::ExtensionSource;
    use crate::server::NoServerCommand;
    use pretty_assertions::assert_eq;

    const FIXTURE: &str = "wasm-fixture";

    /// An extensions dir holding the fixture code extension (`tests/fixtures/wasm-ext`) installed
    /// with `wasm` as its `extension.wasm`, and an index that opts it in or not.
    struct InstalledFixture {
        _data: tempfile::TempDir,
        dir: ExtensionsDir,
    }

    impl InstalledFixture {
        fn new(wasm: &[u8], wasm_enabled: bool) -> Self {
            let data = tempfile::TempDir::new().expect("temp data dir");
            let dir = ExtensionsDir::under_data_dir(data.path());
            let installed = dir.extension(FIXTURE);
            std::fs::create_dir_all(&installed).expect("the installed extension dir");
            std::fs::copy(
                Self::source().join("extension.toml"),
                installed.join("extension.toml"),
            )
            .expect("copy extension.toml");
            std::fs::write(installed.join(EXTENSION_WASM), wasm).expect("write extension.wasm");
            let entry = IndexEntry {
                id: FIXTURE.to_string(),
                version: "0.1.0".to_string(),
                source: ExtensionSource::Path,
                installed_at: 1_760_000_000,
                wasm_enabled,
            };
            let index = Index {
                extensions: [(FIXTURE.to_string(), entry)].into_iter().collect(),
            };
            std::fs::write(
                dir.index(),
                serde_json::to_string(&index).expect("serialise the index"),
            )
            .expect("write index.json");
            Self { _data: data, dir }
        }

        fn source() -> PathBuf {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/wasm-ext")
        }

        fn built_wasm() -> Vec<u8> {
            std::fs::read(Self::source().join(EXTENSION_WASM)).expect("read the fixture's wasm")
        }

        /// One language of the fixture, served as `choice`, its server asked for `kinds`.
        fn language(&self, choice: ServerChoice, kinds: &[&str]) -> (ExtensionManifest, Served) {
            let installed = self.dir.extension(FIXTURE);
            let manifest = ExtensionManifest::parse(
                &std::fs::read_to_string(installed.join("extension.toml")).expect("read manifest"),
            )
            .expect("the fixture manifest parses");
            let language = ExtensionLanguage {
                extension: FIXTURE.to_string(),
                extension_dir: installed,
                name: "Demo TOML".to_string(),
                grammar: "toml".to_string(),
                path_suffixes: vec!["toml".to_string()],
                outline_query: None,
            };
            let kinds = kinds.iter().map(|kind| kind.to_string()).collect();
            (manifest, (language, choice, kinds))
        }

        fn unconfigured(&self, server: &str) -> ServerChoice {
            ServerChoice::Unconfigured(NoServerCommand {
                server: server.to_string(),
                extension: FIXTURE.to_string(),
                settings_file: ExtensionSettings::file(&self.dir),
            })
        }
    }

    /// The choice and kinds of each served language.
    fn choices_and_kinds(served: Vec<Served>) -> Vec<(ServerChoice, Vec<String>)> {
        served
            .into_iter()
            .map(|(_, choice, kinds)| (choice, kinds))
            .collect()
    }

    #[test]
    fn a_wasm_refusal_refuses_the_server_in_its_words_and_keeps_its_code_action_kinds() {
        // Given the fixture opted in, and downloading-ls — whose WASM calls download_file —
        // unconfigured and asked for source.fixAll actions
        let fixture = InstalledFixture::new(&InstalledFixture::built_wasm(), true);
        let language = fixture.language(fixture.unconfigured("downloading-ls"), &["source.fixAll"]);

        // When the installed languages are served
        let served = served_by_wasm(&fixture.dir, vec![language]).expect("served");

        // Then the run refuses the server with the WASM's refusal, and its kinds ride along
        assert_eq!(
            choices_and_kinds(served),
            vec![(
                ServerChoice::Refused(ServerRefusal {
                    server: "downloading-ls".to_string(),
                    reason: format!(
                        "extension {FIXTURE}: language server downloading-ls: {}",
                        download_refusal("downloading-ls")
                    ),
                }),
                vec!["source.fixAll".to_string()]
            )]
        );
    }

    #[test]
    fn a_server_of_an_extension_not_opted_in_stays_unconfigured() {
        // Given the fixture not opted in, with an extension.wasm that would refuse to load
        let fixture = InstalledFixture::new(b"not a component", false);
        let unconfigured = fixture.unconfigured("fake-ls");
        let language = fixture.language(unconfigured.clone(), &[]);

        // When the installed languages are served
        let served = served_by_wasm(&fixture.dir, vec![language]).expect("served");

        // Then K22's refusal stands and the WASM was never read (R1)
        assert_eq!(choices_and_kinds(served), vec![(unconfigured, vec![])]);
    }

    #[test]
    fn with_no_unconfigured_server_not_even_the_index_is_read() {
        // Given the fixture with an index no parser accepts, and a language with no server
        let fixture = InstalledFixture::new(b"not a component", true);
        std::fs::write(fixture.dir.index(), "not json").expect("break index.json");
        let language = fixture.language(ServerChoice::Undeclared, &[]);

        // When the installed languages are served
        let served = served_by_wasm(&fixture.dir, vec![language]);

        // Then nothing was read: the language keeps its choice
        assert_eq!(
            served.map(choices_and_kinds),
            Ok(vec![(ServerChoice::Undeclared, vec![])])
        );
    }

    #[test]
    fn only_zed_extension_api_0_6_is_supported() {
        // Given versions on and around the supported range
        let versions = ["0.6.0", "0.6.7", "0.5.9", "0.7.0", "1.6.0", "0.6", "six"];

        // When each is checked
        let supported: Vec<bool> = versions.iter().map(|v| supports_api_version(v)).collect();

        // Then only 0.6.x is — and what does not parse is not
        assert_eq!(
            supported,
            vec![true, true, false, false, false, false, false]
        );
    }

    #[test]
    fn the_download_refusal_names_the_server_and_its_setting() {
        // Given a server whose extension asked to download it
        let server = "elixir-ls";

        // When the refusal is written
        let refusal = download_refusal(server);

        // Then it says what to install and which setting to write
        assert_eq!(
            refusal,
            "tddy does not download language servers; install elixir-ls and set \
             lsp.elixir-ls.binary.path"
        );
    }

    #[test]
    fn the_unsupported_version_refusal_gives_the_range() {
        // Given an extension built for 0.5.0
        let refusal = WasmCommandError::UnsupportedApiVersion {
            extension: "old-ext".into(),
            version: Some("0.5.0".into()),
        };

        // When it is shown
        let shown = refusal.to_string();

        // Then it names the version and the supported range
        assert_eq!(
            shown,
            "extension old-ext: built for zed_extension_api 0.5.0; tddy supports 0.6.0..<0.7.0"
        );
    }
}
