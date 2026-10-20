//! Test support for the WASM-command suite: the fixture code extension `wasm-fixture`
//! (`tests/fixtures/wasm-ext`, built once from its committed source), installed into a temp
//! extensions dir, and a worktree whose `PATH` holds `fake-ls` — `tddy-lsp`'s deterministic
//! `fake_lsp`, never a real language server.

#![allow(
    dead_code,
    reason = "each test binary uses a different subset of the helpers"
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tddy_lsp::{LanguageId, LaunchSpec, LspAllowList, LspError, LspKey, LspRegistry};
use tddy_task::TaskRegistry;
use tddy_zed_extensions::{
    ExtensionLanguage, ExtensionManifest, ExtensionSettings, ExtensionSource, ExtensionsDir, Index,
    IndexEntry, LibEntry, LibKind, NoServerCommand, ServerChoice, WasmWorktree,
};

/// The fixture extension's id, as its `extension.toml` declares it.
pub const WASM_FIXTURE: &str = "wasm-fixture";

/// The fixture's servers: `which("fake-ls")`, a `download_file` call, and a loop.
pub const FAKE_LS: &str = "fake-ls";
pub const DOWNLOADING_LS: &str = "downloading-ls";
pub const LOOPING_LS: &str = "looping-ls";

fn the_fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/wasm-ext")
}

/// An extensions dir under a temp data dir, holding `wasm-fixture` installed — its
/// `extension.toml` and, unless told otherwise, its `extension.wasm`.
pub struct AnInstalledWasmExtension {
    _data: tempfile::TempDir,
    pub extensions: ExtensionsDir,
}

impl AnInstalledWasmExtension {
    /// `wasm-fixture` installed with the `extension.wasm` built from its source.
    pub fn built_from_its_source() -> Self {
        let installed = Self::without_its_wasm();
        fs::copy(
            the_fixture_dir().join("extension.wasm"),
            installed.extension_dir().join("extension.wasm"),
        )
        .expect("copy the fixture's extension.wasm");
        installed
    }

    /// `wasm-fixture` installed with an `extension.wasm` that is not WebAssembly at all — what
    /// any attempt to compile it would refuse.
    pub fn with_an_unloadable_wasm() -> Self {
        let installed = Self::without_its_wasm();
        fs::write(
            installed.extension_dir().join("extension.wasm"),
            b"not a component",
        )
        .expect("write the unloadable extension.wasm");
        installed
    }

    /// `wasm-fixture` installed with its manifest only.
    pub fn without_its_wasm() -> Self {
        let data = tempfile::TempDir::new().expect("temp data dir");
        let extensions = ExtensionsDir::under_data_dir(data.path());
        let installed = Self {
            _data: data,
            extensions,
        };
        fs::create_dir_all(installed.extension_dir()).expect("the installed extension dir");
        fs::copy(
            the_fixture_dir().join("extension.toml"),
            installed.extension_dir().join("extension.toml"),
        )
        .expect("copy the fixture's extension.toml");
        installed
    }

    /// `installed/wasm-fixture`.
    pub fn extension_dir(&self) -> PathBuf {
        self.extensions.extension(WASM_FIXTURE)
    }

    /// The installed manifest.
    pub fn manifest(&self) -> ExtensionManifest {
        ExtensionManifest::parse(
            &fs::read_to_string(self.extension_dir().join("extension.toml"))
                .expect("read the installed manifest"),
        )
        .expect("the fixture manifest parses")
    }

    /// `index.json` holding the fixture, opted in or not — written directly, so the suite's setup
    /// does not go through the index code it tests.
    pub fn write_index_with_wasm_enabled(&self, wasm_enabled: bool) {
        let index = Index {
            extensions: [(WASM_FIXTURE.to_string(), the_index_entry(wasm_enabled))]
                .into_iter()
                .collect(),
        };
        fs::write(
            self.extensions.index(),
            serde_json::to_string(&index).expect("serialise the index"),
        )
        .expect("write index.json");
    }

    /// `<extensions dir>/settings.json`, as K22 names it in a refusal.
    pub fn settings_file(&self) -> PathBuf {
        ExtensionSettings::file(&self.extensions)
    }

    /// K22's choice for a declared server with no `binary.path`.
    pub fn unconfigured(&self, server: &str) -> ServerChoice {
        ServerChoice::Unconfigured(NoServerCommand {
            server: server.to_string(),
            extension: WASM_FIXTURE.to_string(),
            settings_file: self.settings_file(),
        })
    }
}

/// The fixture's index entry.
pub fn the_index_entry(wasm_enabled: bool) -> IndexEntry {
    IndexEntry {
        id: WASM_FIXTURE.to_string(),
        version: "0.1.0".to_string(),
        source: ExtensionSource::Path,
        installed_at: 1_760_000_000,
        wasm_enabled,
    }
}

/// `[lib]` of a Rust extension built for `zed_extension_api` `version`.
pub fn a_rust_lib_built_for(version: &str) -> LibEntry {
    LibEntry {
        kind: Some(LibKind::Rust),
        version: Some(version.to_string()),
    }
}

/// The language the fixture's servers are declared for; the fixture ships no grammar, and nothing
/// here parses with one.
pub fn demo_toml_of(installed: &AnInstalledWasmExtension) -> ExtensionLanguage {
    ExtensionLanguage {
        extension: WASM_FIXTURE.to_string(),
        extension_dir: installed.extension_dir(),
        name: "Demo TOML".to_string(),
        grammar: "toml".to_string(),
        path_suffixes: vec!["toml".to_string()],
        outline_query: None,
    }
}

/// A temp worktree whose `PATH` is one directory holding `fake-ls` — a link to the fake server.
pub struct AWorktree {
    dir: tempfile::TempDir,
}

impl AWorktree {
    pub fn with_fake_ls_on_its_path() -> Self {
        let dir = tempfile::TempDir::new().expect("temp worktree");
        fs::create_dir_all(dir.path().join("bin")).expect("the worktree's bin dir");
        std::os::unix::fs::symlink(
            env!("CARGO_BIN_EXE_fake_lsp"),
            dir.path().join("bin").join(FAKE_LS),
        )
        .expect("link fake-ls to the fake server");
        Self { dir }
    }

    pub fn root(&self) -> &Path {
        self.dir.path()
    }

    /// Where `which("fake-ls")` finds the fake server.
    pub fn fake_ls(&self) -> String {
        self.dir
            .path()
            .join("bin")
            .join(FAKE_LS)
            .display()
            .to_string()
    }

    /// The worktree as the host is asked about it: its `PATH` is its own `bin`, nothing else.
    pub fn as_wasm_worktree(&self) -> WasmWorktree {
        WasmWorktree {
            id: 1,
            root: self.root().to_path_buf(),
            shell_env: vec![(
                "PATH".to_string(),
                self.dir.path().join("bin").display().to_string(),
            )],
        }
    }
}

/// Start `launch` for the workspace at `root` through the registry a host keeps its servers in;
/// `Ok` once the server answered `initialize`.
pub async fn the_server_started_by(launch: LaunchSpec, root: &Path) -> Result<(), LspError> {
    let language = LanguageId::new("demo-toml");
    let mut allow = LspAllowList::new();
    allow.allow(language.clone(), launch);
    LspRegistry::new(allow, TaskRegistry::new(), Duration::from_secs(60))
        .get_or_spawn(LspKey {
            root: root.to_path_buf(),
            language,
        })
        .await
        .map(|_| ())
}
