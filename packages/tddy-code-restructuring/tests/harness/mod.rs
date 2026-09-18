//! A real cargo workspace, a real rust-analyzer, and the same wiring `tddy-tools restructure` uses.
//!
//! Everything else in this package is a unit test over JSON fixtures, which is the right level for
//! the deciding half of an operation — but it cannot reach the half that asks the server anything.
//! `RustBackend`'s `documentSymbol` + `textDocument/references` implementation, the position
//! encoding it negotiates, and whether an edit it authored actually *compiles* are all only
//! answerable against a live server and a real toolchain. That is what this harness is for.
//!
//! The server is spawned exactly as production spawns it: `LaunchSpec::new("rust-analyzer")`
//! resolved on `PATH` (the nix dev shell supplies it), carrying this library's own
//! [`client_capabilities`] and [`server_settings`], reached through `tddy-lsp`'s registry and
//! [`RustBackend::from_lsp_client`]. No binary path is hardcoded and no capability is restated
//! here — a handshake that drifted from the one the CLI sends would be testing a different client.

// Every test binary compiles the whole harness and uses the part it needs — the move suite never
// renames and the rename suite never moves. Dead code here is a binary not needing a helper, not a
// helper nobody needs.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use tddy_code_restructuring::apply::apply_workspace_edit;
use tddy_code_restructuring::backends::rust::discard;
use tddy_code_restructuring::registry::{LanguageBackend, Workspace};
use tddy_code_restructuring::{
    client_capabilities, server_settings, Anchor, Overlay, Reexport, RefactorKind, RefactorOp,
    WorkspaceEdit,
};
use tddy_lsp::{Language, LaunchSpec, LspAllowList, LspKey, LspRegistry};
use tddy_task::TaskRegistry;
use tokio_util::sync::CancellationToken;

/// How long this harness is willing to wait for rust-analyzer to load the crate graph.
///
/// The library states no such bound any more: a wait ends when the server is ready or when its
/// caller stops waiting, and nothing else. Here the caller is a test, and a test has to stop —
/// three crates with no external dependencies index in seconds locally, the figure is generous
/// because a CI runner is not, and without it a broken server hangs the suite instead of failing
/// with `IndexingIncomplete` and the toolchain it resolved with.
const A_WAIT_A_TEST_CAN_OUTLAST: Duration = Duration::from_secs(180);

/// One rust-analyzer at a time within this binary.
///
/// These suites are load-sensitive: two servers indexing at once on a shared runner is how a
/// generous budget still expires. Under `cargo nextest` each test is its own process and the
/// `rust-analyzer` test group in `.config/nextest.toml` is what holds the line; this is the same
/// rule for a plain `cargo test`, which runs a binary's tests on several threads and never reads
/// that file.
///
/// Tokio's mutex rather than the standard library's: the guard is held across the server spawn,
/// which is an await, and it does not poison — a test that fails holding it leaves the next one
/// free to run and report its own failure.
static ONE_SERVER_AT_A_TIME: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// A three-crate cargo workspace on disk: the crate a module leaves, the crate it moves to, and a
/// dependency the moved code names.
///
/// `shared` is not decoration. It is what makes "both manifests updated" a claim `cargo check` can
/// settle: the moved module names it, so the destination has to gain the dependency, re-anchored on
/// its own directory, or the workspace stops compiling.
pub struct AFixtureWorkspace {
    root: PathBuf,
    // Dropped last, taking the directory with it.
    _directory: tempfile::TempDir,
}

/// A canonicalised temporary workspace with nothing in it yet.
///
/// rust-analyzer answers with canonical paths, and on macOS a temporary directory is reached
/// through a symlink — so a uri it returns would sit "outside" an uncanonicalised root.
pub fn an_empty_fixture() -> AFixtureWorkspace {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let root = directory
        .path()
        .canonicalize()
        .expect("the temporary directory canonicalises");

    AFixtureWorkspace {
        root,
        _directory: directory,
    }
}

pub fn a_workspace_a_module_can_move_across() -> AFixtureWorkspace {
    an_empty_fixture()
        .writing(
            "Cargo.toml",
            "[workspace]\nresolver = \"2\"\nmembers = [\n    \"crates/shared\",\n    \
             \"crates/origin\",\n    \"crates/destination\",\n]\n",
        )
        .writing(
            "crates/shared/Cargo.toml",
            "[package]\nname = \"shared\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .writing(
            "crates/shared/src/lib.rs",
            "//! What both crates depend on.\n\npub struct Clock;\n\nimpl Clock {\n    \
             pub fn now(&self) -> u64 {\n        0\n    }\n}\n",
        )
        .writing(
            "crates/origin/Cargo.toml",
            "[package]\nname = \"origin\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [dependencies]\nshared = { path = \"../shared\" }\n",
        )
        .writing(
            "crates/origin/src/lib.rs",
            "//! The crate the module leaves.\n\npub mod host_registry;\npub mod runtime;\n",
        )
        .writing(
            "crates/origin/src/host_registry.rs",
            "use shared::Clock;\n\npub struct HostRegistry {\n    clock: Clock,\n}\n\n\
             impl HostRegistry {\n    pub fn new() -> Self {\n        \
             Self { clock: Clock }\n    }\n\n    pub fn stamp(&self) -> u64 {\n        \
             self.clock.now()\n    }\n}\n",
        )
        .writing(
            "crates/origin/src/runtime.rs",
            "use crate::host_registry::HostRegistry;\n\npub fn boot() -> u64 {\n    \
             HostRegistry::new().stamp()\n}\n",
        )
        .writing(
            "crates/destination/Cargo.toml",
            "[package]\nname = \"destination\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .writing(
            "crates/destination/src/lib.rs",
            "//! The crate the module moves into.\n\n",
        )
        .tracked_by_git()
}

impl AFixtureWorkspace {
    pub fn path(&self) -> &Path {
        &self.root
    }

    pub fn read(&self, relative: &str) -> String {
        std::fs::read_to_string(self.root.join(relative))
            .unwrap_or_else(|error| panic!("reading {relative}: {error}"))
    }

    pub fn holds(&self, relative: &str) -> bool {
        self.root.join(relative).exists()
    }

    /// Whether the workspace compiles, and what cargo said when it does not.
    ///
    /// This is the assertion the rest of the suite exists to make. Every crate here is local and
    /// dependency-free, so a check needs no network and no registry — it is a compiler run over
    /// four small files, and it is the only thing that can tell an edit that looks right from one
    /// that resolves.
    pub fn cargo_check(&self) -> std::result::Result<(), String> {
        let output = Command::new("cargo")
            .args(["check", "--workspace", "--quiet"])
            .current_dir(&self.root)
            // Its own target directory, so a check here never contends with the build running it.
            .env("CARGO_TARGET_DIR", self.root.join("target"))
            .output()
            .expect("cargo runs");

        if output.status.success() {
            return Ok(());
        }
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }

    /// Overwrite a file in a workspace that already exists.
    ///
    /// The builder's own `writing` consumes `self`, which is right while assembling a fixture and
    /// wrong for a test that needs to vary one file from a shared starting point.
    pub fn rewriting(&self, relative: &str, text: &str) {
        let absolute = self.root.join(relative);
        std::fs::create_dir_all(absolute.parent().expect("a parent directory"))
            .expect("the directory is created");
        std::fs::write(absolute, text).expect("the file is written");
    }

    /// Delete a file, for a test about what happens when it is absent.
    pub fn removing(&self, relative: &str) {
        std::fs::remove_file(self.root.join(relative))
            .unwrap_or_else(|error| panic!("removing {relative}: {error}"));
    }

    fn writing(self, relative: &str, text: &str) -> Self {
        let absolute = self.root.join(relative);
        std::fs::create_dir_all(absolute.parent().expect("a parent directory"))
            .expect("the directory is created");
        std::fs::write(absolute, text).expect("the file is written");
        self
    }

    /// `apply` moves files with `git mv`, which needs them in an index.
    fn tracked_by_git(self) -> Self {
        for arguments in [
            vec!["init", "--quiet"],
            vec!["add", "-A"],
            vec![
                "-c",
                "user.name=fixture",
                "-c",
                "user.email=fixture@example.com",
                "commit",
                "--quiet",
                "-m",
                "baseline",
            ],
        ] {
            let status = Command::new("git")
                .args(&arguments)
                .current_dir(&self.root)
                .status()
                .expect("git runs");
            assert!(status.success(), "git {arguments:?} failed");
        }
        self
    }
}

/// Resolve one operation against a live rust-analyzer and apply what it produced.
///
/// The two halves are deliberately together: an edit that resolves and does not apply is not a
/// working operation, and the tests here assert on the tree afterwards rather than on the edit.
pub async fn performing(fixture: &AFixtureWorkspace, op: RefactorOp) -> WorkspaceEdit {
    let _serialized = ONE_SERVER_AT_A_TIME.lock().await;
    let root = fixture.path().to_path_buf();
    let client = a_rust_analyzer_rooted_at(&root).await;

    let cancel = a_token_cancelled_after(A_WAIT_A_TEST_CAN_OUTLAST);

    tokio::task::spawn_blocking(move || {
        let mut backend = tddy_code_restructuring::backends::rust::RustBackend::from_lsp_client(
            client,
            Some(cancel),
            discard(),
        );
        let overlay = Overlay::default();
        let workspace = Workspace {
            root: &root,
            overlay: &overlay,
        };

        let resolution = backend
            .resolve(&op, &workspace)
            .unwrap_or_else(|error| panic!("resolving {:?}: {error}", op.op));
        apply_workspace_edit(&root, &resolution.edit).expect("the resolved edit applies");
        resolution.edit
    })
    .await
    .expect("the blocking half of the operation joins")
}

/// Resolve one operation and hand back what it produced — including a refusal.
///
/// `performing` panics on a refusal because its tests assert on the tree. A test about *why* an
/// operation refuses needs the error itself, which is what this returns.
pub async fn resolving(
    fixture: &AFixtureWorkspace,
    op: RefactorOp,
) -> Result<WorkspaceEdit, String> {
    let _serialized = ONE_SERVER_AT_A_TIME.lock().await;
    let root = fixture.path().to_path_buf();
    let client = a_rust_analyzer_rooted_at(&root).await;

    let cancel = a_token_cancelled_after(A_WAIT_A_TEST_CAN_OUTLAST);

    tokio::task::spawn_blocking(move || {
        let mut backend = tddy_code_restructuring::backends::rust::RustBackend::from_lsp_client(
            client,
            Some(cancel),
            discard(),
        );
        let overlay = Overlay::default();
        let workspace = Workspace {
            root: &root,
            overlay: &overlay,
        };

        backend
            .resolve(&op, &workspace)
            .map(|resolution| resolution.edit)
            .map_err(|error| error.to_string())
    })
    .await
    .expect("the blocking half of the operation joins")
}

/// rust-analyzer, launched the way `tddy-tools restructure` launches it.
async fn a_rust_analyzer_rooted_at(root: &Path) -> Arc<tddy_lsp::client::LspClient> {
    let mut allow = LspAllowList::new();
    allow.allow(
        Language::Rust,
        LaunchSpec::new("rust-analyzer")
            .with_capabilities(client_capabilities())
            .with_initialization_options(server_settings()),
    );

    let registry = LspRegistry::new(allow, TaskRegistry::new(), A_WAIT_A_TEST_CAN_OUTLAST);
    let service = registry
        .get_or_spawn(LspKey {
            root: root.to_path_buf(),
            language: Language::Rust,
        })
        .await
        .expect("rust-analyzer starts — the nix dev shell puts it on PATH");

    // The client's own default is sized for interactive queries; a request against a cold index
    // routinely outlasts it. `tddy-tools` raises it from `--indexing-budget` for the same reason.
    service
        .client
        .set_request_timeout(A_WAIT_A_TEST_CAN_OUTLAST);

    Arc::clone(&service.client)
}

/// A token this harness cancels once it has waited as long as it is prepared to.
fn a_token_cancelled_after(wait: Duration) -> CancellationToken {
    let cancel = CancellationToken::new();
    let outlasted = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(wait).await;
        outlasted.cancel();
    });
    cancel
}

/// A workspace whose movable module is **nested** — declared by another module's file, not by the
/// crate root.
///
/// This is the shape `source_crate_of` refuses today, and it is the *normal* shape of a subsystem
/// worth extracting: `model_registry/` was chosen as `#unbundle` node 2's opening move precisely
/// because it was the cleanest extraction available, and the operation could not touch a line of it.
///
/// `declared_by_mod_rs` selects which of the two forms Rust 2018 allows for the parent:
/// `src/model_registry.rs`, or `src/model_registry/mod.rs`.
pub fn a_workspace_whose_module_is_nested(declared_by_mod_rs: bool) -> AFixtureWorkspace {
    let fixture = an_empty_fixture();
    let parent = if declared_by_mod_rs {
        "crates/origin/src/model_registry/mod.rs"
    } else {
        "crates/origin/src/model_registry.rs"
    };

    fixture
        .writing(
            "Cargo.toml",
            "[workspace]\nresolver = \"2\"\nmembers = [\n    \"crates/shared\",\n    \
             \"crates/origin\",\n    \"crates/destination\",\n]\n",
        )
        .writing(
            "crates/shared/Cargo.toml",
            "[package]\nname = \"shared\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .writing(
            "crates/shared/src/lib.rs",
            "//! What both crates depend on.\n\npub struct Clock;\n\nimpl Clock {\n    \
             pub fn now(&self) -> u64 {\n        0\n    }\n}\n",
        )
        .writing(
            "crates/origin/Cargo.toml",
            "[package]\nname = \"origin\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [dependencies]\nshared = { path = \"../shared\" }\n",
        )
        .writing(
            "crates/origin/src/lib.rs",
            "//! The crate the nested module leaves.\n\npub mod model_registry;\npub mod runtime;\n",
        )
        .writing(parent, "//! The parent that declares it.\n\npub mod store;\n")
        .writing(
            "crates/origin/src/model_registry/store.rs",
            "use shared::Clock;\n\npub struct Store {\n    clock: Clock,\n}\n\n\
             impl Store {\n    pub fn new() -> Self {\n        \
             Self { clock: Clock }\n    }\n\n    pub fn stamp(&self) -> u64 {\n        \
             self.clock.now()\n    }\n}\n",
        )
        .writing(
            "crates/origin/src/runtime.rs",
            "use crate::model_registry::store::Store;\n\npub fn boot() -> u64 {\n    \
             Store::new().stamp()\n}\n",
        )
        .writing(
            "crates/destination/Cargo.toml",
            "[package]\nname = \"destination\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .writing(
            "crates/destination/src/lib.rs",
            "//! The crate the module moves into.\n\n",
        )
        .tracked_by_git()
}

/// A workspace where the origin keeps a **back-compat `pub use` facade**, and the moving module
/// reaches through it.
///
/// rust-analyzer canonicalises the moving module's `crate::config::Setting` as
/// `origin::config::Setting`, so the cycle refusal reads a re-export as an origin dependency and
/// refuses a move that is in fact clean — `config` is `shared`'s.
pub fn a_workspace_whose_origin_re_exports_what_moves_reaches() -> AFixtureWorkspace {
    an_empty_fixture()
        .writing(
            "Cargo.toml",
            "[workspace]\nresolver = \"2\"\nmembers = [\n    \"crates/shared\",\n    \
             \"crates/origin\",\n    \"crates/destination\",\n]\n",
        )
        .writing(
            "crates/shared/Cargo.toml",
            "[package]\nname = \"shared\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .writing(
            "crates/shared/src/lib.rs",
            "//! Where `config` is actually defined.\n\npub mod config;\n",
        )
        .writing(
            "crates/shared/src/config.rs",
            "pub struct Setting;\n\nimpl Setting {\n    pub fn value(&self) -> u64 {\n        \
             7\n    }\n}\n",
        )
        .writing(
            "crates/origin/Cargo.toml",
            "[package]\nname = \"origin\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [dependencies]\nshared = { path = \"../shared\" }\n",
        )
        .writing(
            "crates/origin/src/lib.rs",
            "//! The crate the module leaves — and a facade it kept.\n\n\
             pub use shared::config;\n\npub mod host_registry;\n",
        )
        .writing(
            "crates/origin/src/host_registry.rs",
            "use crate::config::Setting;\n\npub struct HostRegistry;\n\n\
             impl HostRegistry {\n    pub fn stamp(&self) -> u64 {\n        \
             Setting.value()\n    }\n}\n",
        )
        .writing(
            "crates/destination/Cargo.toml",
            "[package]\nname = \"destination\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .writing(
            "crates/destination/src/lib.rs",
            "//! The crate the module moves into.\n\n",
        )
        .tracked_by_git()
}

/// A workspace whose two modules **reference each other**, and a third they both leave behind.
///
/// This is the shape `move_module_to_crate` cannot move: whichever of the pair goes first, the
/// other's `crate::` path is re-pointed at a crate its module is about to leave, and between the
/// two operations the tree does not compile. `#unbundle` node 3 moved 0 of 4 modules of exactly
/// this shape.
///
/// `limits` is not decoration. It is what makes "a path reaching a module staying behind still
/// reads as the origin" a claim `cargo check` can settle, and it is what the destination has to
/// gain a dependency on — while never gaining one on itself.
pub fn a_workspace_whose_modules_reference_each_other() -> AFixtureWorkspace {
    an_empty_fixture()
        .writing(
            "Cargo.toml",
            "[workspace]\nresolver = \"2\"\nmembers = [\n    \"crates/origin\",\n    \
             \"crates/destination\",\n]\n",
        )
        .writing(
            "crates/origin/Cargo.toml",
            "[package]\nname = \"origin\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .writing(
            "crates/origin/src/lib.rs",
            "//! The crate the entangled pair leaves.\n\npub mod limits;\npub mod spawner;\n             pub mod spawn_worker;\n",
        )
        .writing("crates/origin/src/limits.rs", "pub struct Limit;\n")
        .writing(
            "crates/origin/src/spawner.rs",
            "use crate::limits::Limit;\nuse crate::spawn_worker::Worker;\n\n             pub struct Spawner;\n\nimpl Spawner {\n    pub fn worker(&self) -> Worker {\n                     Worker\n    }\n\n    pub fn limit(&self) -> Limit {\n        Limit\n    }\n}\n",
        )
        .writing(
            "crates/origin/src/spawn_worker.rs",
            "use crate::spawner::Spawner;\n\npub struct Worker;\n\nimpl Worker {\n                 pub fn spawner(&self) -> Spawner {\n        Spawner\n    }\n}\n",
        )
        .writing(
            "crates/destination/Cargo.toml",
            "[package]\nname = \"destination\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .writing(
            "crates/destination/src/lib.rs",
            "//! The crate the set moves into.\n\n",
        )
        .tracked_by_git()
}

/// The cross-crate move of a whole set, anchored on the first module and naming the rest in `also`.
pub fn a_cluster_move_of(modules: &[&str], reexport: Option<Reexport>) -> RefactorOp {
    let mut anchors = modules.iter().map(|module| Anchor::Symbol {
        file: format!("crates/origin/src/{module}.rs"),
        path: (*module).to_string(),
    });

    RefactorOp {
        op: RefactorKind::MoveClusterToCrate,
        anchor: anchors.next().expect("a cluster names at least one module"),
        name: None,
        to: Some("crates/destination".to_string()),
        variant: None,
        with_private_deps: false,
        reexport,
        to_file: false,
        also: anchors.collect(),
    }
}

/// The cross-crate move of a module named by path, with or without a facade.
pub fn a_move_of(
    file: &str,
    path: &str,
    reexport: Option<tddy_code_restructuring::Reexport>,
) -> RefactorOp {
    RefactorOp {
        op: RefactorKind::MoveModuleToCrate,
        anchor: Anchor::Symbol {
            file: file.to_string(),
            path: path.to_string(),
        },
        name: None,
        to: Some("crates/destination".to_string()),
        variant: None,
        with_private_deps: false,
        reexport,
        to_file: false,
        also: Vec::new(),
    }
}

/// Run the operation and return the refusal it produced, or fail saying it did not refuse.
pub async fn refusal_from(fixture: &AFixtureWorkspace, op: RefactorOp) -> String {
    match resolving(fixture, op).await {
        Err(refusal) => refusal,
        Ok(_) => panic!("the operation was expected to refuse, and resolved instead"),
    }
}

/// The cross-crate move of `host_registry`, with or without a facade.
pub fn a_move_of_the_host_registry(
    reexport: Option<tddy_code_restructuring::Reexport>,
) -> RefactorOp {
    RefactorOp {
        op: RefactorKind::MoveModuleToCrate,
        anchor: Anchor::Symbol {
            file: "crates/origin/src/host_registry.rs".to_string(),
            path: "host_registry".to_string(),
        },
        name: None,
        to: Some("crates/destination".to_string()),
        variant: None,
        with_private_deps: false,
        reexport,
        to_file: false,
        also: Vec::new(),
    }
}

/// Renaming a symbol the module declares, which another file in the crate reaches.
pub fn a_rename_of(symbol: &str, to: &str) -> RefactorOp {
    RefactorOp {
        op: RefactorKind::RenameSymbol,
        anchor: Anchor::Symbol {
            file: "crates/origin/src/host_registry.rs".to_string(),
            path: symbol.to_string(),
        },
        name: Some(to.to_string()),
        to: None,
        variant: None,
        with_private_deps: false,
        reexport: None,
        to_file: false,
        also: Vec::new(),
    }
}

/// A `cargo check` failure, said in full.
pub fn assert_compiles(fixture: &AFixtureWorkspace) {
    if let Err(said) = fixture.cargo_check() {
        panic!("the workspace no longer compiles after the operation:\n{said}");
    }
}
