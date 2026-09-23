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
use tddy_code_restructuring::backends::rust::{discard, ServerChatter};
use tddy_code_restructuring::registry::{LanguageBackend, Workspace};
use tddy_code_restructuring::{
    client_capabilities, server_settings, Anchor, Overlay, Position, Reexport, RefactorKind,
    RefactorOp, WorkspaceEdit,
};
use tddy_lsp::{Language, LaunchSpec, LspAllowList, LspKey, LspRegistry, NotificationEvent};
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

/// How far the server has got when the operation is handed to it.
///
/// Two states, because the engine meets both in production and they fail differently. `tddy-tools
/// restructure` usually starts its own server and asks straight away. Against `tddy-index-daemon` it
/// finds a server that is already warm: one that has reported `experimental/serverStatus`
/// `quiescent: true`, which is the daemon's own definition of loaded.
///
/// They behave differently. For a few seconds after its first hover answers, a fresh server reports
/// semantic tokens with no `unresolvedReference` among them, and it does not know a type a build
/// script generates. The import pass reads the first, and the extract-method signature reads the
/// second. A test about a defect seen against a warm index has to run against one, or it proves
/// nothing about that defect.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ServerState {
    /// Handed over as soon as the server has answered its handshake, as `tddy-tools` does cold.
    JustStarted,
    /// Handed over once the server has said it is quiescent, as `tddy-index-daemon` does.
    Settled,
}

/// Resolve one operation against a live rust-analyzer and apply what it produced.
///
/// The two halves are deliberately together: an edit that resolves and does not apply is not a
/// working operation, and the tests here assert on the tree afterwards rather than on the edit.
pub async fn performing(fixture: &AFixtureWorkspace, op: RefactorOp) -> WorkspaceEdit {
    performing_against(fixture, op, ServerState::JustStarted).await
}

/// [`performing`], against a server that has already settled — the warm index the destructure
/// plans were checked against.
pub async fn performing_once_settled(fixture: &AFixtureWorkspace, op: RefactorOp) -> WorkspaceEdit {
    performing_against(fixture, op, ServerState::Settled).await
}

async fn performing_against(
    fixture: &AFixtureWorkspace,
    op: RefactorOp,
    state: ServerState,
) -> WorkspaceEdit {
    let root = fixture.path().to_path_buf();
    let described = format!("{:?}", op.op);
    let edit = resolving_against(fixture, op, state)
        .await
        .unwrap_or_else(|error| panic!("resolving {described}: {error}"));
    apply_workspace_edit(&root, &edit).expect("the resolved edit applies");
    edit
}

/// Resolve one operation and hand back what it produced — including a refusal.
///
/// `performing` panics on a refusal because its tests assert on the tree. A test about *why* an
/// operation refuses needs the error itself, which is what this returns.
pub async fn resolving(
    fixture: &AFixtureWorkspace,
    op: RefactorOp,
) -> Result<WorkspaceEdit, String> {
    resolving_against(fixture, op, ServerState::JustStarted).await
}

async fn resolving_against(
    fixture: &AFixtureWorkspace,
    op: RefactorOp,
    state: ServerState,
) -> Result<WorkspaceEdit, String> {
    let _serialized = ONE_SERVER_AT_A_TIME.lock().await;
    let root = fixture.path().to_path_buf();
    let client = a_rust_analyzer_rooted_at(&root).await;
    if state == ServerState::Settled {
        until_quiescent(&client).await;
    }

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

/// Wait until the server reports itself quiescent, read the way `tddy-index-daemon` reads it.
///
/// Folded through the library's own [`ServerChatter`] rather than by picking `quiescent` out of the
/// JSON here: that fold is published so that there is exactly one reading of the notification.
///
/// The subscription is taken after the handshake, and the server reports quiescence only on the
/// transition. Nothing is lost by that: loading a crate graph takes seconds, and the transition
/// comes after it.
async fn until_quiescent(client: &tddy_lsp::client::LspClient) {
    let mut notifications = client.subscribe_notifications();
    let mut chatter = ServerChatter::default();

    let settled = tokio::time::timeout(A_WAIT_A_TEST_CAN_OUTLAST, async {
        while !chatter.quiescent() {
            match notifications.recv().await {
                NotificationEvent::Received(notification) => {
                    chatter.absorb(&notification);
                }
                NotificationEvent::Lost(_) => {}
                NotificationEvent::Ended => panic!("rust-analyzer exited before it settled"),
            }
        }
    })
    .await;

    settled.expect("rust-analyzer reports itself quiescent within the wait a test can outlast");
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
    refusal_against(fixture, op, ServerState::JustStarted).await
}

/// [`refusal_from`], against a server that has already settled.
pub async fn refusal_once_settled_from(fixture: &AFixtureWorkspace, op: RefactorOp) -> String {
    refusal_against(fixture, op, ServerState::Settled).await
}

async fn refusal_against(
    fixture: &AFixtureWorkspace,
    op: RefactorOp,
    state: ServerState,
) -> String {
    match resolving_against(fixture, op, state).await {
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

/// The file every single-crate fixture below splits.
pub const ORIGIN_LIB: &str = "crates/origin/src/lib.rs";

/// Source text from its lines, one per entry, so a fixture's line numbers can be read off it.
fn source(lines: &[&str]) -> String {
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

/// A workspace of the named crates under `crates/`, with nothing written into them yet.
fn a_workspace_of(members: &[&str]) -> AFixtureWorkspace {
    let listed: Vec<String> = members
        .iter()
        .map(|member| format!("    \"crates/{member}\",\n"))
        .collect();
    an_empty_fixture().writing(
        "Cargo.toml",
        &format!(
            "[workspace]\nresolver = \"2\"\nmembers = [\n{}]\n",
            listed.concat()
        ),
    )
}

fn a_manifest_for(name: &str, dependencies: &str) -> String {
    format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n{dependencies}")
}

/// A crate whose file binds a generated type under an **alias** the server *can* resolve.
///
/// `use … as StartSessionEventKind` is the shape `connection_service.rs` uses for every generated
/// proto type it names. Here the aliased type is an ordinary struct, so the server sees it. This
/// is the case the D8 fix (`alias_target`) was written for.
///
/// Lines 17–19 are a seam that names nothing. Lines 21–23 are a seam that names the alias.
pub fn a_crate_whose_alias_the_server_resolves() -> AFixtureWorkspace {
    a_crate_binding_an_alias_to(&[
        "    // Visible to the server.",
        "    pub struct Event {",
        "        pub id: u32,",
        "    }",
    ])
}

/// The same crate, but the aliased type is one **only the compiler sees**.
///
/// That is what a type generated into `OUT_DIR` is to a server that has not loaded the build
/// script's output. The server reports every use of the alias as unresolved, across the whole file,
/// while `cargo check` builds it. `#[cfg(not(rust_analyzer))]` produces exactly that split without a
/// code generator: rust-analyzer sets `cfg(rust_analyzer)` and the compiler does not.
///
/// This is the E1 trigger. The import pass collects unresolved names from the whole file, finds
/// the alias unresolved outside the new module, and writes the parent's `use … as …` into the
/// module. The name stays unresolved and the pass writes the line again, 512 times.
///
/// Line numbers match [`a_crate_whose_alias_the_server_resolves`]: the `cfg` line takes the place
/// of that fixture's comment.
pub fn a_crate_whose_alias_only_the_compiler_resolves() -> AFixtureWorkspace {
    a_crate_binding_an_alias_to(&[
        "    #[cfg(not(rust_analyzer))]",
        "    pub struct Event {",
        "        pub id: u32,",
        "    }",
    ])
}

fn a_crate_binding_an_alias_to(generated: &[&str]) -> AFixtureWorkspace {
    let mut lines: Vec<&str> = vec![
        "//! The file the seams leave.",
        "",
        "// Stands in for the generated module a build script writes.",
        "mod proto {",
    ];
    lines.extend_from_slice(generated);
    lines.push("}");
    lines.extend([
        "",
        "use crate::proto::Event as StartSessionEventKind;",
        "",
        "pub fn first(kind: &StartSessionEventKind) -> u32 {",
        "    kind.id",
        "}",
        "",
        "fn constant() -> u32 {",
        "    7",
        "}",
        "",
        "fn second(kind: &StartSessionEventKind) -> u32 {",
        "    kind.id + 1",
        "}",
        "",
        "pub fn all(kind: &StartSessionEventKind) -> u32 {",
        "    first(kind) + second(kind) + constant()",
        "}",
    ]);

    a_workspace_of(&["origin"])
        .writing("crates/origin/Cargo.toml", &a_manifest_for("origin", ""))
        .writing(ORIGIN_LIB, &source(&lines))
}

/// A crate whose parent still names, bare, a trait the seam at lines 3–5 moves.
///
/// The assist rewrites a call to a moved function into a path through the new module, but it leaves
/// `impl Named for Thing` and `&dyn Named` as they were. Those names are unresolved in the parent
/// only because the seam moved the trait, so the import pass has to restore them there, with a
/// `use` in the parent rather than one in the module.
pub fn a_crate_whose_parent_names_a_trait_the_seam_moves() -> AFixtureWorkspace {
    a_workspace_of(&["origin"])
        .writing("crates/origin/Cargo.toml", &a_manifest_for("origin", ""))
        .writing(
            ORIGIN_LIB,
            &source(&[
                "//! The file the seam leaves.",
                "",
                "pub trait Named {",
                "    fn name(&self) -> u32;",
                "}",
                "",
                "pub struct Thing;",
                "",
                "impl Named for Thing {",
                "    fn name(&self) -> u32 {",
                "        1",
                "    }",
                "}",
                "",
                "pub fn named(thing: &dyn Named) -> u32 {",
                "    thing.name()",
                "}",
            ]),
        )
}

/// A crate that binds a module through a **grouped** `use`, where one seam holds its only user.
///
/// `use shared::sync::{mpsc, RwLock};` is the shape of `cli_session_manager.rs`'s
/// `use tokio::sync::{broadcast, mpsc, oneshot, watch, RwLock};`. `shared` stands in for `tokio`,
/// and `std::sync::mpsc` is the second path to a module named `mpsc`, as it is for tokio's.
///
/// The seam at lines 11–13 holds the file's only use of `mpsc`. The assist drops `mpsc` from the
/// parent's group, because nothing left there uses it. So by the time the import pass asks which
/// `mpsc` the moved code meant, the file no longer binds it. The one binding left that could decide
/// is `use std::sync::Arc;`, which points at the wrong module.
pub fn a_workspace_whose_parent_binds_a_module_in_a_group() -> AFixtureWorkspace {
    a_workspace_of(&["shared", "origin"])
        .writing("crates/shared/Cargo.toml", &a_manifest_for("shared", ""))
        .writing(
            "crates/shared/src/lib.rs",
            &source(&[
                "//! What stands in for `tokio`.",
                "",
                "pub mod sync {",
                "    pub mod mpsc {",
                "        pub struct Sender;",
                "",
                "        pub fn channel() -> Sender {",
                "            Sender",
                "        }",
                "    }",
                "",
                "    pub struct RwLock;",
                "}",
            ]),
        )
        .writing(
            "crates/origin/Cargo.toml",
            &a_manifest_for(
                "origin",
                "\n[dependencies]\nshared = { path = \"../shared\" }\n",
            ),
        )
        .writing(
            ORIGIN_LIB,
            &source(&[
                "//! The file the seam leaves.",
                "",
                "use std::sync::Arc;",
                "",
                "use shared::sync::{mpsc, RwLock};",
                "",
                "pub fn lock() -> Arc<RwLock> {",
                "    Arc::new(RwLock)",
                "}",
                "",
                "pub fn open_channel() -> mpsc::Sender {",
                "    mpsc::channel()",
                "}",
            ]),
        )
}

/// A crate whose request type a **build script** generates into `OUT_DIR`, and generates slowly.
///
/// `StartSessionRequest` in `tddy-service` is produced by `tonic-build`. In the real workspace
/// that build takes minutes. rust-analyzer answers hover, and so passes the engine's readiness
/// wait, before the build script's output is loaded. Until it is loaded the type does not exist
/// for the server, and "extract into function" writes `req: _` for a parameter of that type. The
/// sleep stands in for the code generator's compile time. It is long enough that the extraction
/// lands inside the window, not after it.
///
/// Lines 10–11 are the statements an extract-method takes.
pub fn a_crate_whose_request_type_a_slow_build_script_generates() -> AFixtureWorkspace {
    a_workspace_of(&["origin"])
        .writing("crates/origin/Cargo.toml", &a_manifest_for("origin", ""))
        .writing(
            "crates/origin/build.rs",
            &source(&[
                "//! Generates the request type, as `tonic-build` would, after a code generator's delay.",
                "",
                "fn main() {",
                "    std::thread::sleep(std::time::Duration::from_secs(15));",
                "    let out = std::env::var(\"OUT_DIR\").expect(\"cargo sets OUT_DIR\");",
                "    std::fs::write(",
                "        format!(\"{out}/session.rs\"),",
                "        \"pub struct StartSessionRequest {\\n    pub session_id: u32,\\n}\\n\",",
                "    )",
                "    .expect(\"the generated module is written\");",
                "}",
            ]),
        )
        .writing(
            ORIGIN_LIB,
            &source(&[
                "//! A service whose request type is generated at build time.",
                "",
                "mod proto {",
                "    include!(concat!(env!(\"OUT_DIR\"), \"/session.rs\"));",
                "}",
                "",
                "use crate::proto::StartSessionRequest;",
                "",
                "pub fn start_session(req: StartSessionRequest) -> u32 {",
                "    let session = req.session_id * 2;",
                "    let resumed = session + req.session_id;",
                "    resumed",
                "}",
            ]),
        )
}

/// A type whose `impl` a seam cuts in half, where a member **left behind** calls one that moves.
///
/// The seam at lines 12–14 takes `doubled`. The assist writes it as
/// `mod … { use super::Gauge; impl Gauge { pub fn doubled … } }`, so it is still an inherent method
/// of `Gauge`. The call `self.doubled()` in `reading` resolves through the type, from anywhere in
/// the crate.
pub fn a_crate_whose_impl_member_calls_one_the_seam_moves() -> AFixtureWorkspace {
    a_crate_whose_gauge_reads(&[
        "    pub fn reading(&self) -> u32 {",
        "        self.doubled() + 1",
        "    }",
        "",
        "    pub fn doubled(&self) -> u32 {",
        "        self.level * 2",
        "    }",
    ])
}

/// [`a_crate_whose_impl_member_calls_one_the_seam_moves`], where the moving method is **private**.
///
/// A private method in the new module is private to that module, so the call left behind in the
/// parent would be `E0624`. For the seam to apply, the method has to stay reachable from where it
/// was called.
pub fn a_crate_whose_impl_member_calls_a_private_one_the_seam_moves() -> AFixtureWorkspace {
    a_crate_whose_gauge_reads(&[
        "    pub fn reading(&self) -> u32 {",
        "        self.doubled() + 1",
        "    }",
        "",
        "    fn doubled(&self) -> u32 {",
        "        self.level * 2",
        "    }",
    ])
}

/// A type whose `impl` a seam cuts in half, where the member that **moves** calls one left behind.
///
/// The seam at lines 12–14 takes `doubled`, which calls the private `base` that stays. A private
/// inherent method is visible to the module that declares it and to that module's descendants, and
/// the new module is one of them.
pub fn a_crate_whose_moving_impl_member_calls_one_left_behind() -> AFixtureWorkspace {
    a_crate_whose_gauge_reads(&[
        "    fn base(&self) -> u32 {",
        "        self.level",
        "    }",
        "",
        "    pub fn doubled(&self) -> u32 {",
        "        self.base() * 2",
        "    }",
    ])
}

fn a_crate_whose_gauge_reads(members: &[&str]) -> AFixtureWorkspace {
    let mut lines: Vec<&str> = vec![
        "//! A type whose `impl` a seam cuts in half.",
        "",
        "pub struct Gauge {",
        "    level: u32,",
        "}",
        "",
        "impl Gauge {",
    ];
    lines.extend_from_slice(members);
    lines.push("}");

    a_workspace_of(&["origin"])
        .writing("crates/origin/Cargo.toml", &a_manifest_for("origin", ""))
        .writing(ORIGIN_LIB, &source(&lines))
}

/// A **trait** `impl` a seam cuts in half, where a member left behind calls the one that moves.
///
/// This one really cannot be cut. The new module would hold a second `impl Meter for Gauge` (E0119),
/// and each half would lack the other's items (E0046). No rewrite of the call can repair that. The
/// seam at lines 17–19 takes `doubled`.
pub fn a_crate_whose_trait_impl_member_calls_a_sibling() -> AFixtureWorkspace {
    a_workspace_of(&["origin"])
        .writing("crates/origin/Cargo.toml", &a_manifest_for("origin", ""))
        .writing(
            ORIGIN_LIB,
            &source(&[
                "//! A trait `impl` a seam cuts in half.",
                "",
                "pub trait Meter {",
                "    fn reading(&self) -> u32;",
                "    fn doubled(&self) -> u32;",
                "}",
                "",
                "pub struct Gauge {",
                "    level: u32,",
                "}",
                "",
                "impl Meter for Gauge {",
                "    fn reading(&self) -> u32 {",
                "        self.doubled() + 1",
                "    }",
                "",
                "    fn doubled(&self) -> u32 {",
                "        self.level * 2",
                "    }",
                "}",
            ]),
        )
}

/// `extract_module` over whole lines of a file, grouping them into an inline `mod name`.
pub fn an_extract_module_of(
    fixture: &AFixtureWorkspace,
    file: &str,
    lines: std::ops::RangeInclusive<u32>,
    name: &str,
) -> RefactorOp {
    an_extraction(
        RefactorKind::ExtractModule,
        a_range_over(fixture, file, lines),
        name,
    )
}

/// `extract_method` over whole lines of a function body, into a function called `name`.
pub fn an_extract_method_of(
    fixture: &AFixtureWorkspace,
    file: &str,
    lines: std::ops::RangeInclusive<u32>,
    name: &str,
) -> RefactorOp {
    an_extraction(
        RefactorKind::ExtractMethod,
        a_range_over(fixture, file, lines),
        name,
    )
}

fn an_extraction(op: RefactorKind, anchor: Anchor, name: &str) -> RefactorOp {
    RefactorOp {
        op,
        anchor,
        name: Some(name.to_string()),
        to: None,
        variant: None,
        with_private_deps: false,
        reexport: None,
        to_file: false,
        also: Vec::new(),
    }
}

/// A one-based range from the first non-blank character of the first line to the end of the last.
///
/// Read off the fixture's own text so a range can never point past a line, or into its indent.
fn a_range_over(
    fixture: &AFixtureWorkspace,
    file: &str,
    lines: std::ops::RangeInclusive<u32>,
) -> Anchor {
    let text = fixture.read(file);
    let line = |number: u32| {
        text.split('\n')
            .nth(number as usize - 1)
            .unwrap_or_else(|| panic!("{file} has no line {number}"))
            .to_string()
    };

    let first = line(*lines.start());
    let last = line(*lines.end());
    let indent = first.len() - first.trim_start().len();

    Anchor::Range {
        file: file.to_string(),
        start: Position {
            line: *lines.start(),
            col: indent as u32 + 1,
        },
        end: Position {
            line: *lines.end(),
            col: last.chars().count() as u32 + 1,
        },
    }
}

/// The text of the inline `mod name { … }` block, from its header to its closing brace.
///
/// Lexical, and enough for fixtures this harness writes: the assist puts the closing brace alone on
/// a line at the `mod` keyword's own indent.
pub fn the_module_named(text: &str, name: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let header = format!("mod {name} {{");
    let opened = lines
        .iter()
        .position(|line| line.trim_start() == header)
        .unwrap_or_else(|| panic!("no `{header}` in:\n{text}"));
    let indent = &lines[opened][..lines[opened].len() - lines[opened].trim_start().len()];
    let closing = format!("{indent}}}");
    let closed = lines[opened..]
        .iter()
        .position(|line| *line == closing)
        .map(|offset| opened + offset)
        .unwrap_or_else(|| panic!("`{header}` is never closed in:\n{text}"));

    lines[opened..=closed].join("\n")
}
