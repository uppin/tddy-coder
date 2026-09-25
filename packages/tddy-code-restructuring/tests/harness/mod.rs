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

use tddy_code_restructuring::apply::{apply_workspace_edit, hash_file};
use tddy_code_restructuring::backends::rust::{discard, ServerChatter};
use tddy_code_restructuring::registry::{LanguageBackend, Workspace};
use tddy_code_restructuring::runner;
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
        self.cargo_check_with(&[])
    }

    /// [`Self::cargo_check`], over test targets too: a `#[cfg(test)]` module is not compiled
    /// without them.
    pub fn cargo_check_all_targets(&self) -> std::result::Result<(), String> {
        self.cargo_check_with(&["--all-targets"])
    }

    fn cargo_check_with(&self, extra: &[&str]) -> std::result::Result<(), String> {
        let output = Command::new("cargo")
            .args(["check", "--workspace", "--quiet"])
            .args(extra)
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
    /// A plan of `ops` written at the workspace root, its snapshot hashing every file they anchor in.
    ///
    /// At the root rather than in a crate, so it is never a file the server indexes or the plan hashes.
    pub fn a_plan_of(&self, ops: &[RefactorOp]) -> PathBuf {
        let mut snapshot = serde_json::Map::new();
        for op in ops {
            let file = op.anchor.file();
            let hash = hash_file(&self.root.join(file)).expect("the anchored file hashes");
            snapshot.insert(file.to_string(), serde_json::Value::String(hash));
        }

        let mut lines = vec![serde_json::json!({ "v": 1, "snapshot": snapshot }).to_string()];
        lines.extend(
            ops.iter()
                .map(|op| serde_json::to_string(op).expect("the operation serialises")),
        );

        let plan = self.root.join("earlier-plan.jsonl");
        std::fs::write(&plan, lines.join("\n") + "\n").expect("the plan is written");
        plan
    }

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
    /// Handed over settled, after an earlier run on the same server has already drained
    /// everything it said — the daemon's second request against a warm root. The status
    /// transitions that run read are not sent again.
    ServedBefore,
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
    if state != ServerState::JustStarted {
        until_quiescent(&client).await;
    }
    if state == ServerState::ServedBefore {
        client.drain_notifications();
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

/// Resolve one operation on a server that a `check --deep` of `earlier` has already run against.
///
/// This is the daemon's shape across requests: one warm server for the root, a backend per run,
/// and nothing between runs but whatever the earlier one left behind. The earlier plan is checked
/// the way `tddy-tools restructure check --deep` checks one, through the runner and its overlay, so
/// each of its operations after the first is resolved against text the tree does not hold. The
/// check writes nothing, and its findings are its own business: what matters is the server it
/// hands on.
pub async fn resolving_after_a_check_of(
    fixture: &AFixtureWorkspace,
    earlier: &[RefactorOp],
    op: RefactorOp,
) -> Result<WorkspaceEdit, String> {
    let _serialized = ONE_SERVER_AT_A_TIME.lock().await;
    let root = fixture.path().to_path_buf();
    let plan = fixture.a_plan_of(earlier);
    let client = a_rust_analyzer_rooted_at(&root).await;
    until_quiescent(&client).await;

    let cancel = a_token_cancelled_after(A_WAIT_A_TEST_CAN_OUTLAST);

    tokio::task::spawn_blocking(move || {
        let options = runner::Options {
            command: runner::Command::Check,
            target: Some(plan),
            deep: true,
            ..runner::Options::default()
        };
        runner::check(&root, options, Some(Arc::clone(&client)), cancel.clone())
            .map_err(|error| format!("the earlier check did not run: {error}"))?;

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
/// The server reports quiescence only on the transition, and a subscription sees only what arrives
/// after it — so a server that settled before this was called would never be heard to. The last
/// status the client kept is folded in first, and it is read *after* subscribing: a status read
/// first could be superseded in the moment before the subscription starts, and that one would be
/// lost; read after, anything newer arrives on the subscription.
async fn until_quiescent(client: &tddy_lsp::client::LspClient) {
    let mut notifications = client.subscribe_notifications();
    let mut chatter = ServerChatter::default();
    if let Some(status) = client.server_status() {
        chatter.absorb(&status);
    }

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

/// [`refusal_from`], against a warm server an earlier run has already read everything from.
pub async fn refusal_from_a_server_served_before(
    fixture: &AFixtureWorkspace,
    op: RefactorOp,
) -> String {
    refusal_against(fixture, op, ServerState::ServedBefore).await
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

/// [`assert_compiles`], over test targets too, for a fixture whose `#[cfg(test)]` module a seam
/// touches.
pub fn assert_compiles_with_its_tests(fixture: &AFixtureWorkspace) {
    if let Err(said) = fixture.cargo_check_all_targets() {
        panic!("the workspace and its tests no longer compile after the operation:\n{said}");
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

/// A crate whose build script fails, so rust-analyzer loads it without what the script generates.
///
/// The shape behind E2 on the real repository, reduced: a code generator that fails inside
/// rust-analyzer's environment. The server still finishes loading and says `quiescent: true`, and
/// reports the failure only through its health.
///
/// Lines 4–5 are statements an extract-method could take; nothing in them needs the build script,
/// so an operation here would otherwise succeed.
pub fn a_crate_whose_build_script_fails() -> AFixtureWorkspace {
    a_workspace_of(&["origin"])
        .writing("crates/origin/Cargo.toml", &a_manifest_for("origin", ""))
        .writing(
            "crates/origin/build.rs",
            &source(&[
                "//! A code generator that cannot run here.",
                "",
                "fn main() {",
                "    panic!(\"protoc is not on PATH\");",
                "}",
            ]),
        )
        .writing(
            ORIGIN_LIB,
            &source(&[
                "//! A crate whose build script fails.",
                "",
                "pub fn doubled(level: u32) -> u32 {",
                "    let twice = level * 2;",
                "    let settled = twice + 1;",
                "    settled",
                "}",
            ]),
        )
}

/// A crate whose function returns early from inside the statements an extract-method would take.
///
/// The shape of `start_session_core` in the destructure's plan 10: a `return Ok(…)` that exits the
/// enclosing function. Lines 4–7 are the range; line 6 is the early return.
pub fn a_crate_whose_function_returns_early() -> AFixtureWorkspace {
    a_workspace_of(&["origin"])
        .writing("crates/origin/Cargo.toml", &a_manifest_for("origin", ""))
        .writing(
            ORIGIN_LIB,
            &source(&[
                "//! A function with an early exit in the middle of its body.",
                "",
                "pub fn level(x: bool) -> Result<u32, String> {",
                "    let base = 2;",
                "    if x {",
                "        return Ok(1);",
                "    }",
                "    Ok(base)",
                "}",
            ]),
        )
}

/// The test binary the compile-gate fixtures move, from `origin` to `destination`.
pub const THE_TEST_BINARY: &str = "crates/origin/tests/golden.rs";

/// A test binary that reads its expected output from a file **beside it**, with `include_str!`.
///
/// `move_test_binary_to_crate` accepts it, moves the file and leaves `golden/expected.txt` where it
/// was, so the moved binary no longer compiles — and nothing but a compiler can say so. Only a
/// `cargo check` that includes test targets builds it at all.
pub fn a_workspace_whose_test_binary_reads_a_file_beside_it() -> AFixtureWorkspace {
    a_workspace_with_a_test_binary(&[
        "//! Reads its expected output from a file beside it.",
        "",
        "#[test]",
        "fn matches_the_golden_output() {",
        "    assert_eq!(include_str!(\"golden/expected.txt\"), \"2\\n\");",
        "}",
    ])
    .writing("crates/origin/tests/golden/expected.txt", "2\n")
    .tracked_by_git()
}

/// A test binary that needs nothing beside it, so moving it leaves a tree that still compiles.
pub fn a_workspace_whose_test_binary_stands_alone() -> AFixtureWorkspace {
    a_workspace_with_a_test_binary(&[
        "//! Needs nothing but itself.",
        "",
        "#[test]",
        "fn doubles() {",
        "    assert_eq!(1 + 1, 2);",
        "}",
    ])
    .tracked_by_git()
}

fn a_workspace_with_a_test_binary(test: &[&str]) -> AFixtureWorkspace {
    a_workspace_of(&["origin", "destination"])
        .writing("crates/origin/Cargo.toml", &a_manifest_for("origin", ""))
        .writing(ORIGIN_LIB, "pub fn level() -> u32 {\n    2\n}\n")
        .writing(THE_TEST_BINARY, &source(test))
        .writing(
            "crates/destination/Cargo.toml",
            &a_manifest_for("destination", ""),
        )
        .writing(
            "crates/destination/src/lib.rs",
            "//! Where the test goes.\n",
        )
}

/// Apply a one-operation plan moving [`THE_TEST_BINARY`] to `destination`, through the library's
/// own `apply` — the entry point `tddy-tools restructure apply` dispatches to.
///
/// The server is the deterministic fake: a test-binary move is authored by the engine and asks the
/// server nothing, so a real rust-analyzer would only add an index nobody reads.
pub async fn applying_a_move_of_the_test_binary(
    fixture: &AFixtureWorkspace,
) -> Result<tddy_code_restructuring::runner::RunSummary, String> {
    let root = fixture.path().to_path_buf();
    let digest = tddy_code_restructuring::apply::hash_file(&root.join(THE_TEST_BINARY))
        .expect("the test binary hashes");
    let plan = root.join("plan.jsonl");
    std::fs::write(
        &plan,
        format!(
            "{{\"v\":1,\"snapshot\":{{\"{THE_TEST_BINARY}\":\"{digest}\"}}}}\n\
             {{\"op\":\"move_test_binary_to_crate\",\"anchor\":{{\"kind\":\"symbol\",\
             \"file\":\"{THE_TEST_BINARY}\",\"path\":\"golden\"}},\"to\":\"crates/destination\"}}\n"
        ),
    )
    .expect("the plan is written");

    let client = a_server_no_operation_asks(&root).await;
    let options = tddy_code_restructuring::runner::Options {
        command: tddy_code_restructuring::runner::Command::Apply,
        target: Some(plan),
        ..tddy_code_restructuring::runner::Options::default()
    };
    tokio::task::spawn_blocking(move || {
        tddy_code_restructuring::runner::apply(
            &root,
            options,
            Some(client),
            CancellationToken::new(),
        )
        .map_err(|error| error.to_string())
    })
    .await
    .expect("the blocking half of the apply joins")
}

/// The deterministic fake language server, for an operation that never asks it anything.
async fn a_server_no_operation_asks(root: &Path) -> Arc<tddy_lsp::client::LspClient> {
    let mut allow = LspAllowList::new();
    allow.allow(
        Language::Rust,
        LaunchSpec::new(env!("CARGO_BIN_EXE_fake_lsp"))
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
        .expect("the fake language server starts");
    Arc::clone(&service.client)
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

/// The non-root module file the relative-import fixtures split: a child of `service`.
pub const HOST_MODULE: &str = "crates/origin/src/service/host.rs";

/// The module that declares the type [`HOST_MODULE`] reaches through `super`.
pub const SERVICE_MODULE: &str = "crates/origin/src/service.rs";

/// A crate whose **non-root** module reaches its parent's type through `use super::Failure;`, where
/// the seam at lines 12–17 of [`HOST_MODULE`] takes a method naming it.
///
/// The shape of `connection_service/svc_spawn_split_agent.rs`, whose `use super::SplitStartFailure;`
/// names a `pub(crate)` enum `connection_service.rs` declares, and whose seam takes a method of an
/// inherent `impl`. The module the seam becomes is a **child** of `host`, so the parent's
/// `super::Failure` is `super::super::Failure` there.
pub fn a_crate_whose_module_imports_its_parent_s_type_through_super() -> AFixtureWorkspace {
    a_crate_whose_host_module_reads(&[
        "//! A module that names its parent's type through `super`.",
        "",
        "use super::Failure;",
        "",
        "pub struct Host;",
        "",
        "impl Host {",
        "    pub fn describe(&self, failure: Failure) -> u32 {",
        "        self.tally(failure) + 1",
        "    }",
        "",
        "    pub(crate) fn tally(&self, failure: Failure) -> u32 {",
        "        match failure {",
        "            Failure::Refused => 1,",
        "            Failure::Timeout => 2,",
        "        }",
        "    }",
        "}",
        "",
        "pub fn described() -> u32 {",
        "    Host.describe(Failure::Timeout)",
        "}",
    ])
}

/// [`a_crate_whose_module_imports_its_parent_s_type_through_super`], where the parent's type is
/// bound under an **alias**: `use super::Failure as HostFailure;`.
pub fn a_crate_whose_module_aliases_its_parent_s_type_through_super() -> AFixtureWorkspace {
    a_crate_whose_host_module_reads(&[
        "//! A module that names its parent's type through `super`, under an alias.",
        "",
        "use super::Failure as HostFailure;",
        "",
        "pub struct Host;",
        "",
        "impl Host {",
        "    pub fn describe(&self, failure: HostFailure) -> u32 {",
        "        self.tally(failure) + 1",
        "    }",
        "",
        "    pub(crate) fn tally(&self, failure: HostFailure) -> u32 {",
        "        match failure {",
        "            HostFailure::Refused => 1,",
        "            HostFailure::Timeout => 2,",
        "        }",
        "    }",
        "}",
        "",
        "pub fn described() -> u32 {",
        "    Host.describe(HostFailure::Timeout)",
        "}",
    ])
}

fn a_crate_whose_host_module_reads(host: &[&str]) -> AFixtureWorkspace {
    a_workspace_of(&["origin"])
        .writing("crates/origin/Cargo.toml", &a_manifest_for("origin", ""))
        .writing(
            ORIGIN_LIB,
            &source(&[
                "//! The crate root.",
                "",
                "mod service;",
                "",
                "pub fn described() -> u32 {",
                "    service::described()",
                "}",
            ]),
        )
        .writing(
            SERVICE_MODULE,
            &source(&[
                "//! The module that owns the type its child module names.",
                "",
                "mod host;",
                "",
                "#[derive(Clone, Copy)]",
                "pub(crate) enum Failure {",
                "    Refused,",
                "    Timeout,",
                "}",
                "",
                "pub fn described() -> u32 {",
                "    host::described()",
                "}",
            ]),
        )
        .writing(HOST_MODULE, &source(host))
}

/// A type whose `impl` a seam cuts in half, where a member left behind calls an **associated
/// function** that moves, as `Self::doubled(…)`.
///
/// The shape of `cli_session_manager.rs`, whose `build_claude_argv` / `build_cursor_argv` take no
/// `self` and are called as `Self::build_claude_argv(…)`. The seam at lines 12–14 takes `doubled`.
/// An associated function is reached through its type wherever its `impl` is, so the call resolves
/// from either side of the cut.
pub fn a_crate_whose_impl_member_calls_an_associated_fn_the_seam_moves() -> AFixtureWorkspace {
    a_crate_whose_gauge_reads_then(
        &[
            "    pub fn reading(&self) -> u32 {",
            "        Self::doubled(self.level) + 1",
            "    }",
            "",
            "    pub fn doubled(level: u32) -> u32 {",
            "        level * 2",
            "    }",
        ],
        &[],
    )
}

/// A type whose associated function the seam at lines 12–14 moves, called **through the type's
/// name** from the file's own `#[cfg(test)] mod tests`: `Gauge::doubled(2)`.
///
/// `cli_session_manager.rs`'s tests call `ClaudeCliSessionManager::build_claude_argv(…)` this way.
pub fn a_crate_whose_tests_call_an_associated_fn_the_seam_moves_through_the_type(
) -> AFixtureWorkspace {
    a_crate_whose_gauge_reads_then(
        &[
            "    pub fn reading(&self) -> u32 {",
            "        self.level + 1",
            "    }",
            "",
            "    pub fn doubled(level: u32) -> u32 {",
            "        level * 2",
            "    }",
        ],
        &[
            "",
            "#[cfg(test)]",
            "mod tests {",
            "    use super::*;",
            "",
            "    #[test]",
            "    fn doubles_the_level() {",
            "        let doubled = Gauge::doubled(2);",
            "        assert_eq!(doubled, 4);",
            "    }",
            "}",
        ],
    )
}

/// The same, called through a **type alias** of the type: `Meter::doubled(2)`, where
/// `pub type Meter = Gauge;`. `ClaudeCliSessionManager` is such an alias.
pub fn a_crate_whose_tests_call_an_associated_fn_the_seam_moves_through_an_alias(
) -> AFixtureWorkspace {
    a_crate_whose_gauge_reads_then(
        &[
            "    pub fn reading(&self) -> u32 {",
            "        self.level + 1",
            "    }",
            "",
            "    pub fn doubled(level: u32) -> u32 {",
            "        level * 2",
            "    }",
        ],
        &[
            "",
            "pub type Meter = Gauge;",
            "",
            "#[cfg(test)]",
            "mod tests {",
            "    use super::*;",
            "",
            "    #[test]",
            "    fn doubles_the_level() {",
            "        let doubled = Meter::doubled(2);",
            "        assert_eq!(doubled, 4);",
            "    }",
            "}",
        ],
    )
}

fn a_crate_whose_gauge_reads_then(members: &[&str], after: &[&str]) -> AFixtureWorkspace {
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
    lines.extend_from_slice(after);

    a_workspace_of(&["origin"])
        .writing("crates/origin/Cargo.toml", &a_manifest_for("origin", ""))
        .writing(ORIGIN_LIB, &source(&lines))
}

/// A file whose own `#[cfg(test)]` module imports, **by name**, a function the seam at lines 7–9
/// moves: `mod tests { use super::base; … }`.
///
/// The shape of `split_session.rs`, whose `mod withdrawal_contract_tests` opens with
/// `use super::split_claude_extra_args;` and whose plan 04 moves that function into `agent_argv`.
pub fn a_crate_whose_test_module_imports_a_function_the_seam_moves() -> AFixtureWorkspace {
    a_crate_whose_tests_reach_base_through("    use super::base;")
}

/// The same, where the test module reaches the function through `use super::*;`.
pub fn a_crate_whose_test_module_globs_a_function_the_seam_moves() -> AFixtureWorkspace {
    a_crate_whose_tests_reach_base_through("    use super::*;")
}

fn a_crate_whose_tests_reach_base_through(import: &str) -> AFixtureWorkspace {
    a_workspace_of(&["origin"])
        .writing("crates/origin/Cargo.toml", &a_manifest_for("origin", ""))
        .writing(
            ORIGIN_LIB,
            &source(&[
                "//! A file whose tests call a function a seam moves.",
                "",
                "pub fn level() -> u32 {",
                "    base() + 1",
                "}",
                "",
                "pub fn base() -> u32 {",
                "    2",
                "}",
                "",
                "#[cfg(test)]",
                "mod tests {",
                import,
                "",
                "    #[test]",
                "    fn reads_the_base() {",
                "        let read = base();",
                "        assert_eq!(read, 2);",
                "    }",
                "}",
            ]),
        )
}
