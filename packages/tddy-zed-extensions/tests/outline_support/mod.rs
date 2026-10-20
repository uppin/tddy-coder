//! Test support for the outline suite: an extensions dir with fixture extensions written straight
//! into the installed layout (no install function is reached, so nothing here waits on
//! `extension-install`), a workspace holding one TOML file, and the backend registry an entry
//! point would resolve its item anchors through.

#![allow(
    dead_code,
    reason = "each test binary uses a different subset of the helpers"
)]

use std::fs;
use std::path::{Path, PathBuf};

use tddy_code_restructuring::{BackendRegistry, Fingerprint, LanguageSupport, Position, Range};
use tddy_zed_extensions::{ExtensionsDir, InstalledLanguages};
use tempfile::TempDir;

/// The file every outline test reads, relative to the workspace root.
pub const APP: &str = "config/app.toml";

/// `config/app.toml`: three tables of pairs, a blank line between tables.
///
/// ```text
///  1 [server]
///  2 host = "localhost"
///  3 port = 8080
///  4
///  5 [database]
///  6 url = "postgres://localhost/app"
///  7 pool = 4
///  8
///  9 [logging]
/// 10 level = "info"
/// ```
pub const APP_TOML: &str = "[server]\nhost = \"localhost\"\nport = 8080\n\n[database]\nurl = \"postgres://localhost/app\"\npool = 4\n\n[logging]\nlevel = \"info\"\n";

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// A fixture extension under `tests/fixtures/`.
#[derive(Debug, Clone, Copy)]
pub enum AFixtureExtension {
    /// `demo-toml`: language `Demo TOML` (id `demo-toml`), suffixes `toml` and `rs`, the vendored
    /// tree-sitter-toml grammar and an `outline.scm`.
    DemoToml,
    /// `broken-grammar`: languages `Broken A` and `Broken B`, both on grammar `broken`, whose
    /// `.wasm` is not WebAssembly.
    BrokenGrammar,
}

impl AFixtureExtension {
    pub fn id(self) -> &'static str {
        match self {
            AFixtureExtension::DemoToml => "demo-toml",
            AFixtureExtension::BrokenGrammar => "broken-grammar",
        }
    }
}

/// A temp tddy data dir whose extensions dir holds the fixture extensions it was given, in Zed's
/// installed layout. Dropping it deletes it.
pub struct AnExtensionsDir {
    _tmp: TempDir,
    pub extensions: ExtensionsDir,
}

pub fn an_extensions_dir_with(installed: &[AFixtureExtension]) -> AnExtensionsDir {
    let tmp = TempDir::new().expect("temp data dir");
    let extensions = ExtensionsDir::under_data_dir(tmp.path());
    for fixture in installed {
        let target = extensions.extension(fixture.id());
        copy_tree(&fixtures().join(fixture.id()), &target);
        if let AFixtureExtension::DemoToml = fixture {
            fs::create_dir_all(target.join("grammars")).expect("grammars dir");
            fs::copy(
                fixtures().join("grammars/tree-sitter-toml/toml.wasm"),
                target.join("grammars/toml.wasm"),
            )
            .expect("copy the vendored grammar");
        }
    }
    AnExtensionsDir {
        _tmp: tmp,
        extensions,
    }
}

impl AnExtensionsDir {
    /// The data dir the extensions dir is under — what `TDDY_DATA_DIR` names.
    pub fn data_dir(&self) -> &Path {
        self._tmp.path()
    }
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("create fixture target");
    for entry in fs::read_dir(from).expect("read fixture dir") {
        let entry = entry.expect("fixture entry");
        let path = entry.path();
        let target = to.join(entry.file_name());
        if path.is_dir() {
            copy_tree(&path, &target);
        } else {
            fs::copy(&path, &target).expect("copy fixture file");
        }
    }
}

/// A workspace holding `config/app.toml` ([`APP_TOML`]) and nothing else — no Cargo package.
pub struct AWorkspace {
    tmp: TempDir,
}

pub fn a_workspace_holding_the_app_config() -> AWorkspace {
    let tmp = TempDir::new().expect("temp workspace");
    fs::create_dir_all(tmp.path().join("config")).expect("config dir");
    fs::write(tmp.path().join(APP), APP_TOML).expect("write the app config");
    AWorkspace { tmp }
}

impl AWorkspace {
    pub fn root(&self) -> &Path {
        self.tmp.path()
    }

    /// Rewrite `config/app.toml`, as an edit after an anchor was taken would.
    pub fn rewrite_the_app_config(&self, text: &str) {
        fs::write(self.root().join(APP), text).expect("rewrite the app config");
    }
}

/// The backends of `languages`' supports, with no server, built over `workspace`'s root — what an
/// entry point resolves an extension language's item anchors through.
pub fn a_backend_registry_of(
    languages: &InstalledLanguages,
    workspace: &AWorkspace,
) -> BackendRegistry {
    let mut registry = BackendRegistry::new();
    for support in &languages.supports {
        registry.register(support.backend(
            tddy_code_restructuring::ServerHandle::none(),
            workspace.root(),
        ));
    }
    registry
}

/// One-based `line:col`.
pub fn at(line: u32, col: u32) -> Position {
    Position { line, col }
}

/// The one-based range `from`..`to`.
pub fn span(from: Position, to: Position) -> Range {
    Range {
        start: from,
        end: to,
    }
}

/// The fingerprint item anchors give lines `first..=last` of [`APP_TOML`] — Rust's: the sha256 of
/// the item's whole lines.
pub fn the_fingerprint_of_lines(first: usize, last: usize) -> Fingerprint {
    let lines: Vec<&str> = APP_TOML.split('\n').collect();
    Fingerprint::of(&lines[first - 1..last].join("\n"))
}
