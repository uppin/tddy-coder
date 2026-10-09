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
use std::time::{Duration, Instant};

use tddy_code_restructuring::apply::{apply_workspace_edit, hash_file};
use tddy_code_restructuring::backends::rust::{discard, ServerChatter};
use tddy_code_restructuring::registry::{LanguageBackend, Workspace};
use tddy_code_restructuring::runner;
use tddy_code_restructuring::spawn_record::SpawnRecorder;
use tddy_code_restructuring::{
    client_capabilities, server_settings, Anchor, Overlay, Position, Reexport, RefactorKind,
    RefactorOp, WorkspaceEdit,
};
use tddy_lsp::{
    Language, LaunchSpec, LspAllowList, LspKey, LspRegistry, NotificationEvent, ProcessOutcome,
    ProcessStart, ProcessToken, SpawnObserver,
};
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

/// The child [`a_workspace_whose_module_has_a_directory_child`]'s `host_registry` declares, in the
/// directory beside its file.
pub const A_DIRECTORY_CHILD: &str = "crates/origin/src/host_registry/clock_face.rs";

/// [`a_workspace_a_module_can_move_across`], whose `host_registry` declares `pub mod clock_face;`
/// with its file in `host_registry/` — the `foo.rs` + `foo/` shape a crate move used to strand
/// (`#reshape` 5/19).
pub fn a_workspace_whose_module_has_a_directory_child() -> AFixtureWorkspace {
    an_empty_fixture()
        .writing("Cargo.toml", THREE_CRATES)
        .writing("crates/shared/Cargo.toml", SHARED_MANIFEST)
        .writing("crates/shared/src/lib.rs", SHARED_LIB)
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
            "pub mod clock_face;\n\nuse shared::Clock;\n\npub struct HostRegistry {\n    \
             clock: Clock,\n}\n\nimpl HostRegistry {\n    pub fn new() -> Self {\n        \
             Self { clock: Clock }\n    }\n\n    pub fn face(&self) -> clock_face::Face {\n        \
             clock_face::Face::of(&self.clock)\n    }\n}\n",
        )
        .writing(
            A_DIRECTORY_CHILD,
            "use shared::Clock;\n\npub struct Face;\n\nimpl Face {\n    \
             pub fn of(_clock: &Clock) -> Self {\n        Face\n    }\n}\n",
        )
        .writing(
            "crates/origin/src/runtime.rs",
            "use crate::host_registry::HostRegistry;\n\npub fn boot() -> HostRegistry {\n    \
             HostRegistry::new()\n}\n",
        )
        .writing("crates/destination/Cargo.toml", DESTINATION_MANIFEST)
        .writing(
            "crates/destination/src/lib.rs",
            "//! The crate the module moves into.\n\n",
        )
        .tracked_by_git()
}

/// The file [`a_workspace_whose_module_file_no_module_declares`] writes and no `mod` declares.
pub const AN_UNDECLARED_MODULE_FILE: &str = "crates/origin/src/clock_face.rs";

/// [`a_workspace_a_module_can_move_across`], plus a module file that no `mod` declares.
///
/// rust-analyzer loads such a file, lists its symbols from the syntax tree and resolves nothing in
/// it, however long it is given: it reports it as `unlinked-file`. The warm index met the same state
/// when a server was never told of a module an earlier apply had created.
pub fn a_workspace_whose_module_file_no_module_declares() -> AFixtureWorkspace {
    a_workspace_a_module_can_move_across().writing(
        AN_UNDECLARED_MODULE_FILE,
        "pub fn face() -> u32 {\n    7\n}\n",
    )
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

    /// Whether rustfmt would leave `relative` as it is, formatting it with edition 2021.
    pub fn is_rustfmt_clean(&self, relative: &str) -> bool {
        Command::new("rustfmt")
            .args(["--check", "--edition", "2021", relative])
            .current_dir(&self.root)
            .output()
            .expect("rustfmt runs")
            .status
            .success()
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

    /// A schema-v2 plan of `ops`: its header carries a hint (hash + update time) per anchored file
    /// rather than a refusing snapshot.
    pub fn a_hinted_plan_of(&self, ops: &[RefactorOp]) -> PathBuf {
        let mut files = serde_json::Map::new();
        for op in ops {
            let file = op.anchor.file();
            let hash = hash_file(&self.root.join(file)).expect("the anchored file hashes");
            files.insert(
                file.to_string(),
                serde_json::json!({ "sha256": hash, "modified": "2026-09-26T00:00:00Z" }),
            );
        }

        let mut lines = vec![serde_json::json!({ "v": 2, "files": files }).to_string()];
        lines.extend(
            ops.iter()
                .map(|op| serde_json::to_string(op).expect("the operation serialises")),
        );

        let plan = self.root.join("hinted-plan.jsonl");
        std::fs::write(&plan, lines.join("\n") + "\n").expect("the plan is written");
        plan
    }

    /// The journal of the run of `plan`, rewritten as a binary that predates plan write-back left it:
    /// no operation ids, and no record that the plan was written back.
    pub fn with_the_journal_of_before_plans_were_kept_current(&self, plan: &Path) {
        let journal = tddy_code_restructuring::state_directory_for_plan(&self.root, plan)
            .expect("the plan has a state directory")
            .join("journal.jsonl");
        let older: Vec<String> = std::fs::read_to_string(&journal)
            .expect("the journal reads")
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("a record parses"))
            .filter(|record| record["status"] != "plan_synced")
            .map(|mut record| {
                record
                    .as_object_mut()
                    .expect("a record is an object")
                    .remove("op_id");
                record.to_string()
            })
            .collect();
        std::fs::write(&journal, older.join("\n") + "\n").expect("the journal is rewritten");
    }

    /// [`Self::a_hinted_plan_of`], whose header also hints at `file`, which is then removed from the
    /// tree — a hinted file that has since been deleted or moved.
    pub fn a_hinted_plan_of_a_tree_that_since_lost(
        &self,
        ops: &[RefactorOp],
        file: &str,
    ) -> PathBuf {
        self.rewriting(file, "pub const GONE: u32 = 0;\n");
        let plan = self.a_hinted_plan_of(ops);
        let hint = serde_json::json!({
            "sha256": hash_file(&self.root.join(file)).expect("the file hashes"),
        });
        let text = std::fs::read_to_string(&plan).expect("the plan reads");
        let (header, operations) = text.split_once('\n').expect("a header line");
        let mut header: serde_json::Value = serde_json::from_str(header).expect("a header");
        header["files"][file] = hint;
        std::fs::write(&plan, format!("{header}\n{operations}")).expect("the plan is written");
        self.removing(file);
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

    /// `relative` taken back out of git's index and committed so, leaving the file on disk: a file
    /// written after the last commit and never added, which `git mv` refuses to move.
    fn untracked(self, relative: &str) -> Self {
        for arguments in [
            vec!["rm", "--cached", "--quiet", relative],
            vec![
                "-c",
                "user.name=fixture",
                "-c",
                "user.email=fixture@example.com",
                "commit",
                "--quiet",
                "-m",
                "untrack",
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
    apply_workspace_edit(&root, &edit, &SpawnRecorder::discard())
        .expect("the resolved edit applies");
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

/// Resolve one operation against a live rust-analyzer and hand back the whole
/// [`Resolution`](tddy_code_restructuring::Resolution) — the edit and what the operation reported
/// about it, its widenings above all. Nothing is applied.
pub async fn resolution_of(
    fixture: &AFixtureWorkspace,
    op: RefactorOp,
) -> Result<tddy_code_restructuring::Resolution, String> {
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
    a_pair_whose_spawner_ends_with("")
}

/// The same pair, `spawner` also holding an item **rust-analyzer never resolves a name in**.
///
/// rust-analyzer sets `cfg(rust_analyzer)` and the compiler does not, so the item is inactive code to
/// the server on every platform while `cargo check` still builds it. That is the shape of
/// `pty_runtime.rs`'s `#[cfg(not(unix))] fn resolve_final_argv_env` on macOS: the outline lists it,
/// and a hover on its name answers `null` however long the server has been ready.
pub fn a_workspace_whose_moving_module_holds_code_the_server_treats_as_inactive(
) -> AFixtureWorkspace {
    a_pair_whose_spawner_ends_with(
        "\n#[cfg(not(rust_analyzer))]\nfn on_other_targets() -> Limit {\n    Limit\n}\n",
    )
}

fn a_pair_whose_spawner_ends_with(tail: &str) -> AFixtureWorkspace {
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
            &format!(
                "{}{tail}",
                "use crate::limits::Limit;\nuse crate::spawn_worker::Worker;\n\n             pub struct Spawner;\n\nimpl Spawner {\n    pub fn worker(&self) -> Worker {\n                     Worker\n    }\n\n    pub fn limit(&self) -> Limit {\n        Limit\n    }\n}\n"
            ),
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
        id: None,
        op: RefactorKind::MoveClusterToCrate,
        anchor: anchors.next().expect("a cluster names at least one module"),
        name: None,
        to: Some("crates/destination".to_string()),
        variant: None,
        with_private_deps: false,
        reexport,
        to_file: false,
        also: anchors.collect(),
        group: None,
        type_: None,
        expr: None,
        order: Vec::new(),
        canonical_paths: false,
        to_type: None,
        callee: None,
    }
}

/// The cross-crate move of a module named by path, with or without a facade.
pub fn a_move_of(
    file: &str,
    path: &str,
    reexport: Option<tddy_code_restructuring::Reexport>,
) -> RefactorOp {
    RefactorOp {
        id: None,
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
        group: None,
        type_: None,
        expr: None,
        order: Vec::new(),
        canonical_paths: false,
        to_type: None,
        callee: None,
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
        id: None,
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
        group: None,
        type_: None,
        expr: None,
        order: Vec::new(),
        canonical_paths: false,
        to_type: None,
        callee: None,
    }
}

/// Renaming a symbol the module declares, which another file in the crate reaches.
pub fn a_rename_of(symbol: &str, to: &str) -> RefactorOp {
    a_rename_in("crates/origin/src/host_registry.rs", symbol, to)
}

/// Renaming a symbol declared in `file`.
pub fn a_rename_in(file: &str, symbol: &str, to: &str) -> RefactorOp {
    RefactorOp {
        id: None,
        op: RefactorKind::RenameSymbol,
        anchor: Anchor::Symbol {
            file: file.to_string(),
            path: symbol.to_string(),
        },
        name: Some(to.to_string()),
        to: None,
        variant: None,
        with_private_deps: false,
        reexport: None,
        to_file: false,
        also: Vec::new(),
        group: None,
        type_: None,
        expr: None,
        order: Vec::new(),
        canonical_paths: false,
        to_type: None,
        callee: None,
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
pub fn source(lines: &[&str]) -> String {
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

/// A crate whose parent module binds `Result` over the prelude: `use crate::{Overlay, Result};`,
/// where `crate::Result` is the crate's one-generic alias, as in the real case.
///
/// Lines 11–17 of the parent are a seam naming that `Result` (`run`) and the prelude's `Option`,
/// which the parent never imports (`pick`).
pub fn a_crate_whose_parent_rebinds_a_prelude_name() -> AFixtureWorkspace {
    a_workspace_of(&["origin"])
        .writing("crates/origin/Cargo.toml", &a_manifest_for("origin", ""))
        .writing(
            ORIGIN_LIB,
            &source(&[
                "//! The crate root, which owns the one-generic `Result`.",
                "",
                "mod entry;",
                "",
                "pub struct Overlay;",
                "",
                "pub struct RestructureError;",
                "",
                "pub type Result<T> = std::result::Result<T, RestructureError>;",
                "",
                "pub fn first() -> Result<u32> {",
                "    entry::first()",
                "}",
            ]),
        )
        .writing(
            ENTRY_MODULE,
            &source(&[
                "//! The module the seam leaves.",
                "",
                "use crate::{Overlay, RestructureError, Result};",
                "",
                "pub fn first() -> Result<u32> {",
                "    Ok(1)",
                "}",
                "",
                "pub fn overlay() -> Overlay {",
                "    Overlay",
                "}",
                "",
                "pub fn run(overlay: &Overlay) -> Result<()> {",
                "    let _ = overlay;",
                "    if false {",
                "        return Err(RestructureError);",
                "    }",
                "    Ok(())",
                "}",
                "",
                "pub fn pick() -> Option<u32> {",
                "    Some(3)",
                "}",
            ]),
        )
        .tracked_by_git()
}

/// The module [`a_crate_whose_parent_rebinds_a_prelude_name`] splits.
pub const ENTRY_MODULE: &str = "crates/origin/src/entry.rs";

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

/// A crate whose function lends its state as a **borrowed view**, `Roster<'a>`, and reads it in its
/// tail.
///
/// The shape of the port-move pilot's `DaemonSessionHost::agent_clone_for`: the host builds a view
/// of its fields and the statements after it read `roster.<field>`. Extracted, the view is a
/// parameter, and rust-analyzer writes its type with the lifetime elided: `roster: Roster<'_>`.
///
/// Lines 14–15 are the tail that reads the view.
pub fn a_crate_whose_function_reads_a_borrowed_view() -> AFixtureWorkspace {
    a_workspace_of(&["origin"])
        .writing("crates/origin/Cargo.toml", &a_manifest_for("origin", ""))
        .writing(
            ORIGIN_LIB,
            &source(&[
                "//! A host that lends its fields as a borrowed view.",
                "",
                "pub struct Roster<'a> {",
                "    pub levels: &'a [u32],",
                "}",
                "",
                "pub struct Host {",
                "    levels: Vec<u32>,",
                "}",
                "",
                "impl Host {",
                "    pub fn highest(&self, floor: u32) -> u32 {",
                "        let roster = Roster { levels: &self.levels };",
                "        let above = roster.levels.iter().filter(|level| **level > floor).count() as u32;",
                "        above + floor",
                "    }",
                "}",
            ]),
        )
}

/// A crate whose method reads one of its host's fields in the middle of an expression.
///
/// The shape of the port-move pilot's `refuse_unready_clone`: `self.clones` is the read an
/// `extract_variable` hoists into a local, so that a later `extract_method` can take it as a
/// parameter. Line 9 holds the read.
pub fn a_crate_whose_method_reads_a_field() -> AFixtureWorkspace {
    a_workspace_of(&["origin"])
        .writing("crates/origin/Cargo.toml", &a_manifest_for("origin", ""))
        .writing(
            ORIGIN_LIB,
            &source(&[
                "//! A host whose method reads one of its fields.",
                "",
                "pub struct Host {",
                "    clones: Vec<u32>,",
                "}",
                "",
                "impl Host {",
                "    pub fn highest(&self) -> u32 {",
                "        self.clones.iter().copied().max().unwrap_or(0)",
                "    }",
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

/// A crate whose function's **last statement** is a `return`, after an early one.
///
/// Lines 4–8 run to the end of the body, and the range ends with `return Ok(base);` rather than a
/// tail expression.
pub fn a_crate_whose_function_ends_with_a_return() -> AFixtureWorkspace {
    a_workspace_of(&["origin"])
        .writing("crates/origin/Cargo.toml", &a_manifest_for("origin", ""))
        .writing(
            ORIGIN_LIB,
            &source(&[
                "//! A function whose last statement is a `return`.",
                "",
                "pub fn level(x: bool) -> Result<u32, String> {",
                "    let base = 2;",
                "    if x {",
                "        return Ok(1);",
                "    }",
                "    return Ok(base);",
                "}",
            ]),
        )
}

/// A crate whose function's tail both propagates an error with `?` and returns early.
///
/// Lines 4–8 are the whole body.
pub fn a_crate_whose_function_propagates_and_returns_early() -> AFixtureWorkspace {
    a_workspace_of(&["origin"])
        .writing("crates/origin/Cargo.toml", &a_manifest_for("origin", ""))
        .writing(
            ORIGIN_LIB,
            &source(&[
                "//! A function that propagates an error and returns early.",
                "",
                "pub fn capped(text: &str) -> Result<u32, std::num::ParseIntError> {",
                "    let value: u32 = text.parse()?;",
                "    if value > 9 {",
                "        return Ok(9);",
                "    }",
                "    Ok(value)",
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

/// [`a_workspace_whose_test_binary_reads_a_file_beside_it`], with an import nothing reads: the moved
/// binary fails to compile and also carries one `unused import` the tidy would have removed.
pub fn a_workspace_whose_test_binary_reads_a_file_beside_it_and_carries_an_unused_import(
) -> AFixtureWorkspace {
    a_workspace_with_a_test_binary(&[
        "//! Reads its expected output from a file beside it, and imports what it never uses.",
        "use std::collections::HashMap;",
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

/// A test binary with an import nothing uses and a use group rustfmt would write differently —
/// what a move leaves behind it: the lint gate and `cargo fmt --check` both fail on it.
pub fn a_workspace_whose_test_binary_carries_an_unused_import_and_an_unformatted_use(
) -> AFixtureWorkspace {
    a_workspace_with_a_test_binary(&[
        "//! Carries an import nothing reads.",
        "use std::collections::HashMap;",
        "use std::fmt::{Debug};",
        "",
        "fn shows(value: impl Debug) -> String {",
        "    format!(\"{value:?}\")",
        "}",
        "",
        "#[test]",
        "fn doubles() {",
        "    assert_eq!(shows(2), \"2\");",
        "}",
    ])
    .tracked_by_git()
}

/// A workspace whose test binary binds a `let mut` it never mutates — the `mut` an extraction copies
/// into the function it writes, which only rustc's `unused_mut` reports.
pub fn a_workspace_whose_test_binary_carries_an_unused_mut() -> AFixtureWorkspace {
    a_workspace_with_a_test_binary(&[
        "//! Binds a `mut` it never needs.",
        "",
        "#[test]",
        "fn doubles() {",
        "    let mut doubled = 2 * 2;",
        "    assert_eq!(doubled, 4);",
        "}",
    ])
    .tracked_by_git()
}

/// The binding [`a_workspace_whose_test_binary_carries_an_unused_mut`] declares `mut`.
pub const THE_UNUSED_MUT: &str = "let mut doubled";

/// The unused import [`a_workspace_whose_test_binary_carries_an_unused_import_and_an_unformatted_use`] holds.
pub const THE_UNUSED_IMPORT: &str = "use std::collections::HashMap;";

/// Where [`THE_TEST_BINARY`] lands.
pub const THE_MOVED_TEST_BINARY: &str = "crates/destination/tests/golden.rs";

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
    moving_the_test_binary(fixture, false).await.0
}

/// [`applying_a_move_of_the_test_binary`], optionally as a `--dry-run`, with every line the run
/// reported to its progress sink.
pub async fn moving_the_test_binary(
    fixture: &AFixtureWorkspace,
    dry_run: bool,
) -> (
    Result<tddy_code_restructuring::runner::RunSummary, String>,
    Vec<String>,
) {
    moving_the_test_binary_recording(
        fixture,
        dry_run,
        SpawnRecorder::discard(),
        CancellationToken::new(),
    )
    .await
}

/// [`moving_the_test_binary`] with the run's process record and its cancellation token in the
/// caller's hands.
pub async fn moving_the_test_binary_recording(
    fixture: &AFixtureWorkspace,
    dry_run: bool,
    spawns: SpawnRecorder,
    cancel: CancellationToken,
) -> (
    Result<tddy_code_restructuring::runner::RunSummary, String>,
    Vec<String>,
) {
    let root = fixture.path().to_path_buf();
    let plan = the_test_binary_move_plan(&root, None);

    let client = a_server_no_operation_asks(&root).await;
    let heard = Arc::new(std::sync::Mutex::new(Vec::new()));
    let keeper = Arc::clone(&heard);
    let options = tddy_code_restructuring::runner::Options {
        command: tddy_code_restructuring::runner::Command::Apply,
        target: Some(plan),
        dry_run,
        progress: Arc::new(move |line: &str| {
            keeper.lock().expect("lines").push(line.to_string());
        }),
        spawns,
        ..tddy_code_restructuring::runner::Options::default()
    };
    let outcome = tokio::task::spawn_blocking(move || {
        tddy_code_restructuring::runner::apply(&root, options, Some(client), cancel)
            .map_err(|error| error.to_string())
    })
    .await
    .expect("the blocking half of the apply joins");
    let said = heard.lock().expect("lines").clone();
    (outcome, said)
}

/// Write the one-operation plan moving [`THE_TEST_BINARY`] to `destination` — in the transactional
/// group `group` when one is named — and return its path.
fn the_test_binary_move_plan(root: &Path, group: Option<&str>) -> PathBuf {
    let digest = tddy_code_restructuring::apply::hash_file(&root.join(THE_TEST_BINARY))
        .expect("the test binary hashes");
    let grouped = group
        .map(|group| format!(",\"group\":\"{group}\""))
        .unwrap_or_default();
    let plan = root.join("plan.jsonl");
    std::fs::write(
        &plan,
        format!(
            "{{\"v\":1,\"snapshot\":{{\"{THE_TEST_BINARY}\":\"{digest}\"}}}}\n\
             {{\"op\":\"move_test_binary_to_crate\",\"anchor\":{{\"kind\":\"symbol\",\
             \"file\":\"{THE_TEST_BINARY}\",\"path\":\"golden\"}},\"to\":\"crates/destination\"\
             {grouped}}}\n"
        ),
    )
    .expect("the plan is written");
    plan
}

/// [`moving_the_test_binary_recording`], with the move the one member of the transactional group
/// `group` — so the group's end-of-group compile gate runs.
pub async fn moving_the_test_binary_in_a_group_recording(
    fixture: &AFixtureWorkspace,
    group: &str,
    spawns: SpawnRecorder,
) -> Result<tddy_code_restructuring::runner::RunSummary, String> {
    let root = fixture.path().to_path_buf();
    let plan = the_test_binary_move_plan(&root, Some(group));
    let client = a_server_no_operation_asks(&root).await;
    let options = tddy_code_restructuring::runner::Options {
        command: tddy_code_restructuring::runner::Command::Apply,
        target: Some(plan),
        spawns,
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

/// A `check` of the plan moving [`THE_TEST_BINARY`] to `destination` — `--deep` when `deep` — over
/// the deterministic fake server, as the details of its findings.
pub async fn checking_a_move_of_the_test_binary(
    fixture: &AFixtureWorkspace,
    deep: bool,
) -> Result<Vec<String>, String> {
    let root = fixture.path().to_path_buf();
    let plan = the_test_binary_move_plan(&root, None);
    let client = a_server_no_operation_asks(&root).await;
    let options = tddy_code_restructuring::runner::Options {
        command: tddy_code_restructuring::runner::Command::Check,
        target: Some(plan),
        deep,
        ..tddy_code_restructuring::runner::Options::default()
    };
    tokio::task::spawn_blocking(move || {
        tddy_code_restructuring::runner::check(
            &root,
            options,
            Some(client),
            CancellationToken::new(),
        )
        .map(|findings| findings.into_iter().map(|finding| finding.detail).collect())
        .map_err(|error| error.to_string())
    })
    .await
    .expect("the blocking half of the check joins")
}

/// The test binary of [`a_workspace_whose_test_binary_stands_alone`], on disk but never added to
/// git — a file written after the last commit, which `git mv` will not move.
pub fn a_workspace_whose_test_binary_is_untracked() -> AFixtureWorkspace {
    a_workspace_whose_test_binary_stands_alone().untracked(THE_TEST_BINARY)
}

/// The destination crate's root, which [`a_workspace_with_a_test_binary`] writes.
pub const DESTINATION_LIB: &str = "crates/destination/src/lib.rs";

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

/// A workspace whose `origin` crate has a build script that never finishes, so a `cargo check` of
/// it blocks for as long as it is allowed to: the shape of the incident that made a run's
/// processes worth recording.
pub fn a_workspace_whose_origin_build_script_never_finishes() -> AFixtureWorkspace {
    a_workspace_with_a_test_binary(&[
        "//! Needs nothing but itself.",
        "",
        "#[test]",
        "fn doubles() {",
        "    assert_eq!(1 + 1, 2);",
        "}",
    ])
    .writing(
        "crates/origin/build.rs",
        "fn main() {\n    std::thread::sleep(std::time::Duration::from_secs(60));\n}\n",
    )
    .tracked_by_git()
}

/// A process that was started, with how it ended once it has.
pub type KeptProcess = (ProcessStart, Option<ProcessOutcome>);

/// A [`SpawnObserver`] that keeps what it is told, in the order it is told it.
///
/// A token is an index into what was kept, so a start and its end find each other again.
#[derive(Clone, Default)]
pub struct CollectedSpawns {
    kept: Arc<std::sync::Mutex<Vec<KeptProcess>>>,
}

impl CollectedSpawns {
    /// Every process started, each with how it ended, or `None` while it has not.
    pub fn processes(&self) -> Vec<KeptProcess> {
        self.kept.lock().expect("collected spawns").clone()
    }

    /// The recorder a run hands to its `Options`.
    pub fn recorder(&self) -> SpawnRecorder {
        SpawnRecorder::new(Arc::new(self.clone()))
    }
}

impl SpawnObserver for CollectedSpawns {
    fn started(&self, process: &ProcessStart) -> ProcessToken {
        let mut kept = self.kept.lock().expect("collected spawns");
        kept.push((process.clone(), None));
        ProcessToken(kept.len() as u64 - 1)
    }

    fn ended(&self, token: ProcessToken, outcome: &ProcessOutcome) {
        let mut kept = self.kept.lock().expect("collected spawns");
        kept[token.0 as usize].1 = Some(outcome.clone());
    }
}

/// Cancel `cancel` once `spawns` holds a process, or after `within` when none ever starts.
///
/// The deadline is what keeps a run that records nothing from waiting on a start that never
/// comes: it is cancelled either way, and what the record then holds is the test's to judge.
pub fn cancelling_once_a_process_has_started(
    spawns: &CollectedSpawns,
    cancel: &CancellationToken,
    within: Duration,
) -> std::thread::JoinHandle<()> {
    let spawns = spawns.clone();
    let cancel = cancel.clone();
    std::thread::spawn(move || {
        let deadline = Instant::now() + within;
        while spawns.processes().is_empty() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        cancel.cancel();
    })
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

/// `extract_variable` over the first occurrence of `expression` on one line, into a binding called
/// `name`.
///
/// The columns are read off the fixture's own text, so the range covers the expression exactly.
pub fn an_extract_variable_of(
    fixture: &AFixtureWorkspace,
    file: &str,
    line: u32,
    expression: &str,
    name: &str,
) -> RefactorOp {
    let text = fixture.read(file);
    let written = text
        .split('\n')
        .nth(line as usize - 1)
        .unwrap_or_else(|| panic!("{file} has no line {line}"));
    let at = written
        .find(expression)
        .unwrap_or_else(|| panic!("line {line} of {file} does not hold `{expression}`"));
    let col = written[..at].chars().count() as u32 + 1;

    an_extraction(
        RefactorKind::ExtractVariable,
        Anchor::Range {
            file: file.to_string(),
            start: Position { line, col },
            end: Position {
                line,
                col: col + expression.chars().count() as u32,
            },
        },
        name,
    )
}

fn an_extraction(op: RefactorKind, anchor: Anchor, name: &str) -> RefactorOp {
    RefactorOp {
        id: None,
        op,
        anchor,
        name: Some(name.to_string()),
        to: None,
        variant: None,
        with_private_deps: false,
        reexport: None,
        to_file: false,
        also: Vec::new(),
        group: None,
        type_: None,
        expr: None,
        order: Vec::new(),
        canonical_paths: false,
        to_type: None,
        callee: None,
    }
}

/// A one-based range from the first non-blank character of the first line to the end of the last.
///
/// Read off the fixture's own text so a range can never point past a line, or into its indent.
pub fn a_range_over(
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

/// The text of `fn name`, from its `fn` keyword to the brace that closes its body.
///
/// Lexical, and enough for fixtures this harness writes: braces are counted outside string
/// literals and line comments.
pub fn the_function_named(text: &str, name: &str) -> String {
    let header = format!("fn {name}(");
    let start = text
        .find(&header)
        .unwrap_or_else(|| panic!("no `{header}` in:\n{text}"));
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut at = start;
    let mut in_string = false;
    while at < bytes.len() {
        match bytes[at] {
            b'\\' if in_string => at += 1,
            b'"' => in_string = !in_string,
            b'/' if !in_string && bytes.get(at + 1) == Some(&b'/') => {
                at = text[at..].find('\n').map_or(bytes.len(), |end| at + end);
                continue;
            }
            b'{' if !in_string => depth += 1,
            b'}' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return text[start..=at].to_string();
                }
            }
            _ => {}
        }
        at += 1;
    }
    panic!("`{header}` is never closed in:\n{text}")
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

/// A private struct and the private function returning it, side by side, with a file-local
/// `#[cfg(test)]` module that calls the function and reads a field of what it returns. The tests
/// never write the type's name, so no path reference to it exists — it escapes through the signature.
///
/// `struct W` and `fn make` are lines 3-9.
pub fn a_crate_whose_tests_read_a_field_of_a_moved_functions_private_return_type(
) -> AFixtureWorkspace {
    a_workspace_of(&["origin"])
        .writing("crates/origin/Cargo.toml", &a_manifest_for("origin", ""))
        .writing(
            ORIGIN_LIB,
            &source(&[
                "//! A function whose return type no path names.",
                "",
                "struct W {",
                "    line: String,",
                "}",
                "",
                "fn make() -> W {",
                "    W { line: \"x\".to_string() }",
                "}",
                "",
                "#[cfg(test)]",
                "mod tests {",
                "    use super::*;",
                "",
                "    #[test]",
                "    fn reads_a_field() {",
                "        assert_eq!(make().line, \"x\");",
                "    }",
                "}",
            ]),
        )
}

/// A private helper and its only caller, side by side; nothing else in the file mentions either.
///
/// `helper` and `caller` are lines 3-9.
pub fn a_crate_whose_private_helper_has_only_its_moved_caller() -> AFixtureWorkspace {
    a_workspace_of(&["origin"])
        .writing("crates/origin/Cargo.toml", &a_manifest_for("origin", ""))
        .writing(
            ORIGIN_LIB,
            &source(&[
                "//! A helper that travels with its only caller.",
                "",
                "fn helper() -> u32 {",
                "    2",
                "}",
                "",
                "pub fn caller() -> u32 {",
                "    helper() + 1",
                "}",
            ]),
        )
}

/// A module file whose `pub(super)` helper the crate root calls, beside a `pub(crate)` function and
/// a private user of the helper. Nothing else mentions them.
///
/// The shape of `imports.rs`, whose `pub(super)` helpers its parent `rust.rs` calls. `helper`,
/// `shared` and `private_user` are lines 3-13 of [`OUTER_MODULE`].
pub fn a_crate_whose_module_declares_a_function_visible_in_its_parent() -> AFixtureWorkspace {
    a_workspace_of(&["origin"])
        .writing("crates/origin/Cargo.toml", &a_manifest_for("origin", ""))
        .writing(
            ORIGIN_LIB,
            &source(&[
                "//! The parent of `outer`, which calls what `outer` shows it.",
                "",
                "mod outer;",
                "",
                "pub fn total() -> u32 {",
                "    outer::helper() + outer::shared() + outer::combined()",
                "}",
            ]),
        )
        .writing(
            OUTER_MODULE,
            &source(&[
                "//! Visible in its parent, and in the crate.",
                "",
                "pub(super) fn helper() -> u32 {",
                "    1",
                "}",
                "",
                "pub(crate) fn shared() -> u32 {",
                "    2",
                "}",
                "",
                "pub(super) fn combined() -> u32 {",
                "    helper() + shared()",
                "}",
            ]),
        )
        .tracked_by_git()
}

/// A module file whose `pub(super)` struct has a `pub(super)` field the crate root reads.
///
/// `Gauge` and `gauge` are lines 3-9 of [`OUTER_MODULE`].
pub fn a_crate_whose_struct_has_a_field_visible_in_its_parent() -> AFixtureWorkspace {
    a_workspace_of(&["origin"])
        .writing("crates/origin/Cargo.toml", &a_manifest_for("origin", ""))
        .writing(
            ORIGIN_LIB,
            &source(&[
                "//! The parent of `outer`, which reads a field `outer` shows it.",
                "",
                "mod outer;",
                "",
                "pub fn total() -> u32 {",
                "    outer::gauge().level",
                "}",
            ]),
        )
        .writing(
            OUTER_MODULE,
            &source(&[
                "//! A struct with a field visible in its parent.",
                "",
                "pub(super) struct Gauge {",
                "    pub(super) level: u32,",
                "}",
                "",
                "pub(super) fn gauge() -> Gauge {",
                "    Gauge { level: 3 }",
                "}",
            ]),
        )
        .tracked_by_git()
}

/// A module file whose last function reaches its surroundings through inline `super::`, `self::`
/// and `crate::` paths, beside a `sibling` that stays behind.
///
/// `caller` is lines 7-9 of [`OUTER_MODULE`]; `super::` from `outer` is the crate root.
pub fn a_crate_whose_moved_function_calls_through_relative_paths() -> AFixtureWorkspace {
    a_workspace_of(&["origin"])
        .writing("crates/origin/Cargo.toml", &a_manifest_for("origin", ""))
        .writing(
            ORIGIN_LIB,
            &source(&[
                "//! The parent of `outer`, which also owns `top_level`.",
                "",
                "mod outer;",
                "",
                "pub fn top_level() -> u32 {",
                "    5",
                "}",
                "",
                "pub fn total() -> u32 {",
                "    outer::caller() + outer::sibling()",
                "}",
            ]),
        )
        .writing(
            OUTER_MODULE,
            &source(&[
                "//! Reaches the root, itself and the crate by path.",
                "",
                "pub(crate) fn sibling() -> u32 {",
                "    1",
                "}",
                "",
                "pub(crate) fn caller() -> u32 {",
                "    super::top_level() + crate::top_level() + self::sibling()",
                "}",
            ]),
        )
        .tracked_by_git()
}

/// A crate whose module `outer` holds `widest` and, after it, `user`, which calls `widest`.
///
/// Two seams of one plan: the first moves `widest` into `outer::visibility`, which leaves `user`
/// calling `visibility::widest()` and `outer` declaring `mod visibility;`. The second moves `user`
/// into `outer::facade`, a sibling of `visibility`, where that path no longer names anything.
pub fn a_crate_whose_second_seam_calls_what_the_first_moved() -> AFixtureWorkspace {
    a_workspace_of(&["origin"])
        .writing("crates/origin/Cargo.toml", &a_manifest_for("origin", ""))
        .writing(
            ORIGIN_LIB,
            &source(&[
                "mod outer;",
                "",
                "pub fn total() -> u32 {",
                "    outer::total()",
                "}",
            ]),
        )
        .writing(
            OUTER_MODULE,
            &source(&[
                "//! `user` calls `widest`; each is moved into a module of its own.",
                "",
                "pub(crate) struct Member {",
                "    pub(crate) visibility: u32,",
                "}",
                "",
                "pub(crate) fn user(member: &Member) -> bool {",
                "    member.visibility != 0 && widest() > 1",
                "}",
                "",
                "pub(crate) fn widest() -> u32 {",
                "    3",
                "}",
                "",
                "pub(crate) fn total() -> u32 {",
                "    u32::from(user(&Member { visibility: 1 }))",
                "}",
            ]),
        )
        .tracked_by_git()
}

/// `extract_module` of the functions of `outer` named `items` (each with its text), written to a
/// file of its own as `name`.
pub fn an_extract_of_outer_functions_into_a_file(items: &[(&str, &str)], name: &str) -> RefactorOp {
    let anchor = Anchor::Items {
        file: OUTER_MODULE.to_string(),
        items: items
            .iter()
            .map(|(item, _)| {
                tddy_code_restructuring::ItemPath::parse(&format!("origin::outer::{item}"))
                    .expect("the item path parses")
            })
            .collect(),
        fingerprints: items
            .iter()
            .map(|(_, text)| tddy_code_restructuring::Fingerprint::of(text))
            .collect(),
    };
    let mut seam = an_extraction(RefactorKind::ExtractModule, anchor, name);
    seam.to_file = true;
    seam
}

/// The module file [`a_crate_whose_module_declares_a_function_visible_in_its_parent`] splits.
pub const OUTER_MODULE: &str = "crates/origin/src/outer.rs";

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

/// The file the item-anchor fixture's items live in.
pub const WORKFLOW: &str = "crates/stacks/src/workflow.rs";

/// `Stack::new` exactly as the item-anchor fixture writes it — the text its fingerprint covers.
pub const STACK_NEW: &str = "    pub fn new() -> Self {\n        let items = Vec::new();\n        \
                             let depth = items.len();\n        Stack { items, depth }\n    }";

/// `Queue::new`, the second inherent `new` in the same file.
pub const QUEUE_NEW: &str =
    "    pub fn new() -> Self {\n        let items = Vec::with_capacity(4);\n        \
                             Queue { items }\n    }";

/// One crate, `stacks`, whose `workflow` module holds two types that each have an inherent `new`,
/// and two trait impls of `Stack` that each define `fmt` — every ambiguity an item path must be
/// able to name its way out of.
///
/// `Stack::new` sits on lines 7–11; its body's two `let` lines are lines 2–3 relative to it.
pub fn a_crate_with_two_inherent_news_and_two_fmts() -> AFixtureWorkspace {
    an_empty_fixture()
        .writing(
            "Cargo.toml",
            "[workspace]\nresolver = \"2\"\nmembers = [\"crates/stacks\"]\n",
        )
        .writing(
            "crates/stacks/Cargo.toml",
            "[package]\nname = \"stacks\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .writing("crates/stacks/src/lib.rs", "pub mod workflow;\n")
        .writing(
            WORKFLOW,
            &format!(
                "pub struct Stack {{\n    items: Vec<u32>,\n    depth: usize,\n}}\n\n\
                 impl Stack {{\n{STACK_NEW}\n}}\n\n\
                 pub struct Queue {{\n    items: Vec<u32>,\n}}\n\n\
                 impl Queue {{\n{QUEUE_NEW}\n}}\n\n\
                 impl std::fmt::Display for Stack {{\n    \
                 fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {{\n        \
                 write!(f, \"{{}}\", self.depth)\n    }}\n}}\n\n\
                 impl std::fmt::Debug for Stack {{\n    \
                 fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {{\n        \
                 write!(f, \"{{:?}}\", self.items)\n    }}\n}}\n"
            ),
        )
        .tracked_by_git()
}

/// An `item` anchor into `item` in `file`, fingerprinted over `item_text` and carrying `hint`.
pub fn an_item_anchor(
    file: &str,
    item: &str,
    item_text: &str,
    relative: Option<(Position, Position)>,
    hint: Option<Position>,
) -> Anchor {
    Anchor::Item {
        item: tddy_code_restructuring::ItemPath::parse(item).expect("the item path parses"),
        file: file.to_string(),
        start: relative.map(|(start, _)| start),
        end: relative.map(|(_, end)| end),
        fingerprint: tddy_code_restructuring::Fingerprint::of(item_text),
        hint,
    }
}

/// One-based `line:col`.
pub fn at(line: u32, col: u32) -> Position {
    Position { line, col }
}

/// `extract_method` addressed at `anchor`, into a function called `name`.
pub fn an_extract_method_at(anchor: Anchor, name: &str) -> RefactorOp {
    an_extraction(RefactorKind::ExtractMethod, anchor, name)
}

/// Write `ops` as a plan with a v1 header over every file they anchor in, then apply it through the
/// runner against a live rust-analyzer — the path `tddy-tools restructure apply` takes.
pub async fn applying_a_plan_of(
    fixture: &AFixtureWorkspace,
    ops: &[RefactorOp],
) -> Result<tddy_code_restructuring::runner::RunSummary, String> {
    let plan = fixture.a_plan_of(ops);
    applying_the_plan_at(fixture, plan).await
}

/// Apply the plan file at `plan` through the runner against a live rust-analyzer.
pub async fn applying_the_plan_at(
    fixture: &AFixtureWorkspace,
    plan: PathBuf,
) -> Result<tddy_code_restructuring::runner::RunSummary, String> {
    applying_the_plan_with(fixture, plan, |_| {}).await
}

/// [`applying_the_plan_at`], with `adjust` given the run's options before it starts — for a run
/// that stops early, resumes, or listens to what it reports.
pub async fn applying_the_plan_with(
    fixture: &AFixtureWorkspace,
    plan: PathBuf,
    adjust: impl FnOnce(&mut runner::Options),
) -> Result<tddy_code_restructuring::runner::RunSummary, String> {
    let _serialized = ONE_SERVER_AT_A_TIME.lock().await;
    let root = fixture.path().to_path_buf();
    let client = a_rust_analyzer_rooted_at(&root).await;
    let cancel = a_token_cancelled_after(A_WAIT_A_TEST_CAN_OUTLAST);
    let mut options = runner::Options {
        command: runner::Command::Apply,
        target: Some(plan),
        ..runner::Options::default()
    };
    adjust(&mut options);
    tokio::task::spawn_blocking(move || {
        runner::apply(&root, options, Some(client), cancel).map_err(|error| error.to_string())
    })
    .await
    .expect("the blocking half of the apply joins")
}

/// What a `check` of the plan at `plan` found, as the lines a finding reads as — the static pass
/// when `deep` is false, which needs no server, and the pass through rust-analyzer when it is true.
pub async fn checking_the_plan(
    fixture: &AFixtureWorkspace,
    plan: PathBuf,
    deep: bool,
) -> Result<Vec<String>, String> {
    checking_the_plan_with(fixture, plan, deep, |_| {}).await
}

/// [`checking_the_plan`], with `adjust` given the run's options before it starts — for a check
/// that listens to the account it gives.
pub async fn checking_the_plan_with(
    fixture: &AFixtureWorkspace,
    plan: PathBuf,
    deep: bool,
    adjust: impl FnOnce(&mut runner::Options),
) -> Result<Vec<String>, String> {
    let root = fixture.path().to_path_buf();
    let mut options = runner::Options {
        command: runner::Command::Check,
        target: Some(plan),
        deep,
        ..runner::Options::default()
    };
    adjust(&mut options);
    let found = if deep {
        let _serialized = ONE_SERVER_AT_A_TIME.lock().await;
        let client = a_rust_analyzer_rooted_at(&root).await;
        let cancel = a_token_cancelled_after(A_WAIT_A_TEST_CAN_OUTLAST);
        tokio::task::spawn_blocking(move || runner::check(&root, options, Some(client), cancel))
            .await
            .expect("the blocking half of the check joins")
    } else {
        runner::check(&root, options, None, CancellationToken::new())
    };
    found
        .map(|findings| findings.into_iter().map(|finding| finding.detail).collect())
        .map_err(|error| error.to_string())
}

/// A progress sink that keeps every line it is given, and the handle to read them back.
pub fn a_sink_that_keeps_what_it_hears() -> (
    tddy_code_restructuring::backends::rust::ProgressSink,
    Arc<std::sync::Mutex<Vec<String>>>,
) {
    let heard = Arc::new(std::sync::Mutex::new(Vec::new()));
    let keeper = Arc::clone(&heard);
    let sink: tddy_code_restructuring::backends::rust::ProgressSink =
        Arc::new(move |line: &str| {
            keeper
                .lock()
                .expect("the sink's lines are readable")
                .push(line.to_string());
        });
    (sink, heard)
}

/// Resolve `item` in `file` through rust-analyzer's outline.
pub async fn resolving_the_item(
    fixture: &AFixtureWorkspace,
    file: &str,
    item: &str,
) -> Result<tddy_code_restructuring::item_anchor::ResolvedItem, String> {
    let path = tddy_code_restructuring::ItemPath::parse(item).map_err(|error| error.to_string())?;
    let file = file.to_string();
    with_a_rust_backend(fixture, move |backend| {
        use tddy_code_restructuring::item_anchor::ItemResolver;
        backend
            .resolve_item(&file, &path)
            .map_err(|error| error.to_string())
    })
    .await
}

/// Run `work` against a `RustBackend` over a live rust-analyzer rooted at the fixture.
async fn with_a_rust_backend<T: Send + 'static>(
    fixture: &AFixtureWorkspace,
    work: impl FnOnce(&mut tddy_code_restructuring::backends::rust::RustBackend) -> T + Send + 'static,
) -> T {
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
        work(&mut backend)
    })
    .await
    .expect("the blocking half joins")
}

/// What `restructure anchors <file> --items …` / `--at …` emits, through the runner.
pub async fn the_anchor_command_emits(
    fixture: &AFixtureWorkspace,
    file: &str,
    items: &[&str],
    at: Option<tddy_code_restructuring::Range>,
) -> Result<Anchor, String> {
    let _serialized = ONE_SERVER_AT_A_TIME.lock().await;
    let root = fixture.path().to_path_buf();
    let client = a_rust_analyzer_rooted_at(&root).await;
    let cancel = a_token_cancelled_after(A_WAIT_A_TEST_CAN_OUTLAST);
    let options = runner::Options {
        command: runner::Command::Anchors,
        target: Some(PathBuf::from(file)),
        items: items.iter().map(|item| item.to_string()).collect(),
        at,
        ..runner::Options::default()
    };
    tokio::task::spawn_blocking(move || {
        runner::item_anchors(&root, options, Some(client), cancel)
            .map_err(|error| error.to_string())
    })
    .await
    .expect("the blocking half joins")
}

/// Apply the plan `key` names from `store`, through the runner, against a live rust-analyzer.
pub async fn applying_from_the_store(
    fixture: &AFixtureWorkspace,
    store: tddy_code_restructuring::plan_store::PlanStore,
    key: tddy_code_restructuring::plan_store::PlanKey,
) -> (
    tddy_code_restructuring::plan_store::PlanStore,
    Result<tddy_code_restructuring::runner::RunSummary, String>,
) {
    let _serialized = ONE_SERVER_AT_A_TIME.lock().await;
    let root = fixture.path().to_path_buf();
    let client = a_rust_analyzer_rooted_at(&root).await;
    let cancel = a_token_cancelled_after(A_WAIT_A_TEST_CAN_OUTLAST);
    tokio::task::spawn_blocking(move || {
        let mut store = store;
        let options = runner::Options {
            command: runner::Command::Apply,
            ..runner::Options::default()
        };
        let outcome =
            runner::apply_from_store(&root, &mut store, &key, options, Some(client), cancel)
                .map_err(|error| error.to_string());
        (store, outcome)
    })
    .await
    .expect("the blocking half of the apply joins")
}

/// A committed workspace holding exactly `files` — for a test whose shape no shared fixture has.
pub fn a_workspace_holding_files(files: &[(&str, &str)]) -> AFixtureWorkspace {
    let mut fixture = an_empty_fixture();
    for (path, text) in files {
        fixture = fixture.writing(path, text);
    }
    fixture.tracked_by_git()
}

/// A three-crate workspace manifest over `shared`, `origin` and `destination`.
pub const THREE_CRATES: &str =
    "[workspace]\nresolver = \"2\"\nmembers = [\n    \"crates/shared\",\n    \
                                \"crates/origin\",\n    \"crates/destination\",\n]\n";

/// `shared`'s manifest and root, which every three-crate fixture carries unchanged.
pub const SHARED_MANIFEST: &str =
    "[package]\nname = \"shared\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
pub const SHARED_LIB: &str = "pub struct Clock;\n";

/// `destination`'s manifest with no dependencies.
pub const DESTINATION_MANIFEST: &str =
    "[package]\nname = \"destination\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";

/// `origin`'s manifest depending on `shared` and on `destination`.
pub const ORIGIN_OVER_BOTH: &str =
    "[package]\nname = \"origin\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
                                    [dependencies]\nshared = { path = \"../shared\" }\n\
                                    destination = { path = \"../destination\" }\n";

/// Assert the workspace passes `cargo clippy --workspace --all-targets -- -D warnings` — CI's lint
/// gate, which a move that compiles can still leave red.
pub fn assert_lints_clean(fixture: &AFixtureWorkspace) {
    let output = Command::new("cargo")
        .args([
            "clippy",
            "--workspace",
            "--all-targets",
            "--quiet",
            "--",
            "-D",
            "warnings",
        ])
        .current_dir(fixture.path())
        .env("CARGO_TARGET_DIR", fixture.path().join("target"))
        .output()
        .expect("cargo clippy runs");
    assert!(
        output.status.success(),
        "the workspace does not lint clean:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Re-resolve the store's anchors in `files` through a live rust-analyzer, as the daemon does when
/// the tree changes underneath it.
pub async fn re_resolving_in_the_store(
    fixture: &AFixtureWorkspace,
    store: tddy_code_restructuring::plan_store::PlanStore,
    files: Vec<String>,
) -> (
    tddy_code_restructuring::plan_store::PlanStore,
    Result<(), String>,
) {
    with_a_rust_backend(fixture, move |backend| {
        let mut store = store;
        let outcome = store
            .reresolve_files(&files, backend)
            .map_err(|error| error.to_string());
        (store, outcome)
    })
    .await
}

/// `restructure snapshot` for an item-anchored plan: re-resolve it once and write it back.
pub async fn rebasing_the_plan_file(
    fixture: &AFixtureWorkspace,
    plan: PathBuf,
) -> Result<Vec<tddy_code_restructuring::plan_store::OpStaleness>, String> {
    let root = fixture.path().to_path_buf();
    with_a_rust_backend(fixture, move |backend| {
        tddy_code_restructuring::plan_store::rebase_plan_file(&root, &plan, backend)
            .map_err(|error| error.to_string())
    })
    .await
}

/// What a live rust-analyzer answers to each `(method, params)` in turn, once the index is
/// quiescent — all of them from the one server, in order, so a probe of two requests pays for one
/// crate-graph load.
///
/// Raw on purpose: this is how a test asks the server what the engine is entitled to assume about
/// the shape of an answer, without the engine in between.
pub async fn what_the_server_answers(
    fixture: &AFixtureWorkspace,
    questions: &[(&str, serde_json::Value)],
) -> Vec<serde_json::Value> {
    let _serialized = ONE_SERVER_AT_A_TIME.lock().await;
    let client = a_rust_analyzer_rooted_at(fixture.path()).await;
    until_quiescent(&client).await;

    let mut answers = Vec::new();
    for (method, params) in questions {
        let answer = client
            .request_raw(method, params.clone())
            .await
            .unwrap_or_else(|error| panic!("the server failed `{method}`: {error}"));
        answers.push(answer);
    }
    answers
}

// ---------------------------------------------------------------------------------------------
// Multi-seam plans (`#reshape` 2): several `extract_module`s cut out of one file, each later seam
// reaching what an earlier one wrote.
// ---------------------------------------------------------------------------------------------

/// A crate whose module `outer` holds `PtyHandle`, whose `send_input` calls the private
/// `strip_resize` below it — the shape of gap J of the 2026-09-24 apply-gaps record.
///
/// Lines 3–11 are the first seam (`PtyHandle` and its `impl`); lines 13–15 the second
/// (`strip_resize`). After the first, `pty_handle.rs` reaches `strip_resize` in the parent.
pub fn a_crate_whose_pty_handle_calls_strip_resize() -> AFixtureWorkspace {
    a_crate_whose_outer_module_reads(&[
        "//! `PtyHandle::send_input` calls `strip_resize`; each is moved into a module of its own.",
        "",
        "pub(crate) struct PtyHandle {",
        "    pub(crate) data: Vec<u8>,",
        "}",
        "",
        "impl PtyHandle {",
        "    pub(crate) fn send_input(&self) -> usize {",
        "        strip_resize(&self.data)",
        "    }",
        "}",
        "",
        "fn strip_resize(data: &[u8]) -> usize {",
        "    data.len()",
        "}",
        "",
        "pub(crate) fn total() -> usize {",
        "    PtyHandle { data: vec![1, 2] }.send_input()",
        "}",
    ])
}

/// The lines of [`a_crate_whose_pty_handle_calls_strip_resize`]'s first seam.
pub const THE_PTY_HANDLE_AND_ITS_IMPL: std::ops::RangeInclusive<u32> = 3..=11;
/// The lines of [`a_crate_whose_pty_handle_calls_strip_resize`]'s second seam.
pub const STRIP_RESIZE: std::ops::RangeInclusive<u32> = 13..=15;
/// The file the first seam of either pty fixture writes.
pub const THE_PTY_HANDLE_FILE: &str = "crates/origin/src/outer/pty_handle.rs";
/// The file the `strip_resize` seam of either pty fixture writes.
pub const THE_RESIZE_FILE: &str = "crates/origin/src/outer/resize.rs";

/// A crate whose module `outer` holds the private `relative_from` and, after it, `moving`, which
/// calls it bare — the shape of the `replacement` / `relative_from` row of the 2026-09-18 record.
///
/// The first seam's assist rewrites the call in the parent to `paths::relative_from`, so the second
/// seam carries a path through a sibling module, which the import pass binds as `use super::paths;`.
///
/// Lines 3–5 are the first seam (`relative_from`); lines 7–9 the second (`moving`).
pub fn a_crate_whose_mover_calls_relative_from() -> AFixtureWorkspace {
    a_crate_whose_outer_module_reads(&[
        "//! `moving` calls `relative_from`; each is moved into a module of its own.",
        "",
        "fn relative_from(base: &str, path: &str) -> usize {",
        "    path.len() - base.len()",
        "}",
        "",
        "pub(crate) fn moving(path: &str) -> usize {",
        "    relative_from(\"/\", path)",
        "}",
        "",
        "pub(crate) fn total() -> usize {",
        "    moving(\"/ab\")",
        "}",
    ])
}

/// The lines of [`a_crate_whose_mover_calls_relative_from`]'s first seam.
pub const RELATIVE_FROM: std::ops::RangeInclusive<u32> = 3..=5;
/// The lines of [`a_crate_whose_mover_calls_relative_from`]'s second seam.
pub const MOVING: std::ops::RangeInclusive<u32> = 7..=9;
/// The file the second seam of [`a_crate_whose_mover_calls_relative_from`] writes.
pub const THE_MOVING_FILES_FILE: &str = "crates/origin/src/outer/moving_files.rs";

/// Three seams that reach one another, as `#carve` 3's eight did: `PtyHandle` (lines 3–11) and
/// `refusals` (lines 13–15) both call `strip_resize` (lines 17–19). The parent keeps a test module
/// that reaches `total` through `use super::*`.
///
/// No later seam's lines name what an earlier seam moves: the assist rewrites such a line in the
/// parent (`&PtyHandle` → `&pty_handle::PtyHandle`), and the position ledger then refuses the later
/// anchor as fallen inside text an earlier operation removed.
pub fn a_crate_cut_three_ways_like_carve_3() -> AFixtureWorkspace {
    a_crate_whose_outer_module_reads(&[
        "//! Three seams reach one another, as `#carve` 3's eight did.",
        "",
        "pub(crate) struct PtyHandle {",
        "    pub(crate) data: Vec<u8>,",
        "}",
        "",
        "impl PtyHandle {",
        "    pub(crate) fn send_input(&self) -> usize {",
        "        strip_resize(&self.data)",
        "    }",
        "}",
        "",
        "pub(crate) fn refusals(data: &[u8]) -> bool {",
        "    strip_resize(data) > 4",
        "}",
        "",
        "fn strip_resize(data: &[u8]) -> usize {",
        "    data.len()",
        "}",
        "",
        "pub(crate) fn total() -> usize {",
        "    let handle = PtyHandle { data: vec![1, 2] };",
        "    handle.send_input() + usize::from(refusals(&handle.data))",
        "}",
        "",
        "#[cfg(test)]",
        "mod tests {",
        "    use super::*;",
        "",
        "    #[test]",
        "    fn totals_the_input() {",
        "        assert_eq!(total(), 2);",
        "    }",
        "}",
    ])
}

/// The lines of [`a_crate_cut_three_ways_like_carve_3`]'s `refusals` seam.
pub const THE_REFUSALS: std::ops::RangeInclusive<u32> = 13..=15;
/// The lines of [`a_crate_cut_three_ways_like_carve_3`]'s `strip_resize` seam.
pub const STRIP_RESIZE_AFTER_THE_REFUSALS: std::ops::RangeInclusive<u32> = 17..=19;

fn a_crate_whose_outer_module_reads(lines: &[&str]) -> AFixtureWorkspace {
    a_workspace_of(&["origin"])
        .writing("crates/origin/Cargo.toml", &a_manifest_for("origin", ""))
        .writing(
            ORIGIN_LIB,
            &source(&[
                "mod outer;",
                "",
                "pub fn total() -> usize {",
                "    outer::total()",
                "}",
            ]),
        )
        .writing(OUTER_MODULE, &source(lines))
        .tracked_by_git()
}

/// `extract_module` of `lines` of [`OUTER_MODULE`] into a file of its own named `name`, with the
/// facade `reexport` asks for.
pub fn a_seam_of_outer(
    fixture: &AFixtureWorkspace,
    lines: std::ops::RangeInclusive<u32>,
    name: &str,
    reexport: Reexport,
) -> RefactorOp {
    let mut seam = an_extract_module_of(fixture, OUTER_MODULE, lines, name);
    seam.to_file = true;
    seam.reexport = Some(reexport);
    seam
}

/// The `visibility:` lines of a run's account, in the order it printed them.
pub fn the_widenings_in(account: &[String]) -> Vec<String> {
    account
        .iter()
        .filter(|line| line.trim_start().starts_with("visibility:"))
        .cloned()
        .collect()
}

/// An `apply` of `ops`, through the runner against a live rust-analyzer, with the account it gave.
pub async fn applying_keeping_the_account(
    fixture: &AFixtureWorkspace,
    ops: &[RefactorOp],
    dry_run: bool,
) -> (
    Result<tddy_code_restructuring::runner::RunSummary, String>,
    Vec<String>,
) {
    let (sink, heard) = a_sink_that_keeps_what_it_hears();
    let plan = fixture.a_plan_of(ops);
    let applied = applying_the_plan_with(fixture, plan, |options| {
        options.dry_run = dry_run;
        options.account = sink;
    })
    .await;
    let account = heard.lock().expect("the account is readable").clone();
    (applied, account)
}

/// A `check --deep` of `ops`, through the runner against a live rust-analyzer, with the account it
/// gave beside its findings.
pub async fn checking_deep_keeping_the_account(
    fixture: &AFixtureWorkspace,
    ops: &[RefactorOp],
) -> (Result<Vec<String>, String>, Vec<String>) {
    let (sink, heard) = a_sink_that_keeps_what_it_hears();
    let plan = fixture.a_plan_of(ops);
    let found = checking_the_plan_with(fixture, plan, true, |options| {
        options.account = sink;
    })
    .await;
    let account = heard.lock().expect("the account is readable").clone();
    (found, account)
}

/// What a run that writes nothing would leave in each file: `ops` resolved one after another through
/// one backend, one position ledger and one overlay — exactly as `check --deep` and `apply --dry-run`
/// resolve a plan — against a live rust-analyzer.
pub struct ResolvedInOrder {
    root: PathBuf,
    overlay: Overlay,
}

impl ResolvedInOrder {
    /// The projected text of `relative`: the overlay's when an operation wrote it, the disk's else.
    pub fn text(&self, relative: &str) -> String {
        self.overlay
            .read(&self.root, Path::new(relative))
            .unwrap_or_else(|error| panic!("reading the projected {relative}: {error}"))
    }
}

/// See [`ResolvedInOrder`]. Every anchor must already be in snapshot coordinates (a range).
pub async fn resolving_in_order(
    fixture: &AFixtureWorkspace,
    ops: &[RefactorOp],
) -> Result<ResolvedInOrder, String> {
    let _serialized = ONE_SERVER_AT_A_TIME.lock().await;
    let root = fixture.path().to_path_buf();
    let client = a_rust_analyzer_rooted_at(&root).await;
    let cancel = a_token_cancelled_after(A_WAIT_A_TEST_CAN_OUTLAST);
    let ops = ops.to_vec();

    tokio::task::spawn_blocking(move || {
        let mut backend = tddy_code_restructuring::backends::rust::RustBackend::from_lsp_client(
            client,
            Some(cancel),
            discard(),
        );
        let mut ledger = tddy_code_restructuring::PositionLedger::new();
        let mut overlay = Overlay::default();
        for op in &ops {
            let at = ledger.translate_op(op).map_err(|error| error.to_string())?;
            let resolution = backend
                .resolve(
                    &at,
                    &Workspace {
                        root: &root,
                        overlay: &overlay,
                    },
                )
                .map_err(|error| error.to_string())?;
            ledger.record(&resolution.edit);
            overlay
                .record(&root, &resolution.edit)
                .map_err(|error| error.to_string())?;
        }
        Ok(ResolvedInOrder { root, overlay })
    })
    .await
    .expect("the blocking half of the resolution joins")
}

/// The module the facade fixtures below split.
pub const PARENT_MODULE: &str = "crates/origin/src/parent.rs";

/// A crate whose `parent` module holds two documented, derived `pub struct`s the crate root
/// re-exports publicly — the shape `journal.rs` / `journal/group.rs` had when `#live-plan 10/15`
/// split them. Lines 7–17 are the two structs.
pub fn a_crate_whose_root_re_exports_documented_pub_items_of_parent() -> AFixtureWorkspace {
    a_workspace_of(&["origin"])
        .writing("crates/origin/Cargo.toml", &a_manifest_for("origin", ""))
        .writing(
            ORIGIN_LIB,
            &source(&[
                "//! Re-exports what `parent` declares.",
                "",
                "pub mod parent;",
                "",
                "pub use parent::{OpenGroup, PreImage};",
            ]),
        )
        .writing(
            PARENT_MODULE,
            &source(&[
                "//! A journal and its groups.",
                "",
                "pub fn record() -> u32 {",
                "    1",
                "}",
                "",
                "/// The bytes of one file before a group touched it.",
                "#[derive(Debug, Clone)]",
                "pub struct PreImage {",
                "    pub path: String,",
                "}",
                "",
                "/// A group that has not finished.",
                "#[derive(Debug)]",
                "pub struct OpenGroup {",
                "    pub members: Vec<usize>,",
                "}",
            ]),
        )
        .tracked_by_git()
}

/// A crate whose `parent` module holds `count`, which `total` calls, and `statements`, a `pub fn`
/// nothing calls. Lines 3–9 are `count` and `statements`.
pub fn a_crate_whose_parent_holds_an_unreferenced_pub_fn() -> AFixtureWorkspace {
    a_workspace_of(&["origin"])
        .writing("crates/origin/Cargo.toml", &a_manifest_for("origin", ""))
        .writing(
            ORIGIN_LIB,
            &source(&["//! Holds `parent`.", "", "pub mod parent;"]),
        )
        .writing(
            PARENT_MODULE,
            &source(&[
                "//! Counts and statements.",
                "",
                "pub fn count() -> usize {",
                "    3",
                "}",
                "",
                "pub fn statements() -> Vec<u32> {",
                "    vec![1, 2]",
                "}",
                "",
                "pub fn total() -> usize {",
                "    count() + 1",
                "}",
            ]),
        )
        .tracked_by_git()
}

/// A crate whose `parent` module holds `part`, which production code calls, and `checked`, which
/// only `parent`'s own test module calls. Lines 3–9 are `part` and `checked`.
pub fn a_crate_whose_parent_tests_alone_call_a_helper() -> AFixtureWorkspace {
    a_workspace_of(&["origin"])
        .writing("crates/origin/Cargo.toml", &a_manifest_for("origin", ""))
        .writing(
            ORIGIN_LIB,
            &source(&["//! Holds `parent`.", "", "pub mod parent;"]),
        )
        .writing(
            PARENT_MODULE,
            &source(&[
                "//! A total and its parts.",
                "",
                "pub(crate) fn part() -> u32 {",
                "    2",
                "}",
                "",
                "pub(crate) fn checked() -> u32 {",
                "    1",
                "}",
                "",
                "pub fn total() -> u32 {",
                "    part() + 1",
                "}",
                "",
                "#[cfg(test)]",
                "mod tests {",
                "    use super::*;",
                "",
                "    #[test]",
                "    fn checks() {",
                "        assert_eq!(checked(), 1);",
                "    }",
                "}",
            ]),
        )
        .tracked_by_git()
}

/// The module [`a_workspace_whose_module_the_origin_still_reaches`] moves.
pub const THE_ROSTER: &str = "crates/origin/src/roster.rs";
/// Where [`THE_ROSTER`] lands.
pub const THE_MOVED_ROSTER: &str = "crates/destination/src/roster.rs";
/// The module staying behind that names [`THE_ROSTER`]'s type and calls its method.
pub const THE_LEDGER: &str = "crates/origin/src/ledger.rs";
/// Where [`THE_LEDGER`] lands when it moves with the roster.
pub const THE_MOVED_LEDGER: &str = "crates/destination/src/ledger.rs";

/// `roster`'s lines as written before any move: a `pub(crate)` struct with a `pub(crate)` and a private
/// field, an inherent `impl` with two `pub(crate)` methods and a private one, a trait `impl`, a
/// `pub(crate)` free function the origin calls and one only the module itself uses.
pub const ROSTER_BEFORE: &[&str] = &[
    "//! The roster.",
    "",
    "pub(crate) struct AgentRoster {",
    "    pub(crate) rev: u64,",
    "    secret: u64,",
    "}",
    "",
    "impl AgentRoster {",
    "    pub(crate) fn new() -> Self {",
    "        Self { rev: 1, secret: 0 }",
    "    }",
    "",
    "    pub(crate) fn broadcast(&self) -> u64 {",
    "        self.rev + self.internal()",
    "    }",
    "",
    "    fn internal(&self) -> u64 {",
    "        self.secret + only_the_roster_uses()",
    "    }",
    "}",
    "",
    "impl Default for AgentRoster {",
    "    fn default() -> Self {",
    "        Self::new()",
    "    }",
    "}",
    "",
    "pub(crate) fn started_roster_rev(roster: &AgentRoster) -> u64 {",
    "    roster.rev",
    "}",
    "",
    "pub(crate) fn only_the_roster_uses() -> u64 {",
    "    0",
    "}",
];

/// A three-crate workspace whose `origin` keeps naming, from `runtime` and `ledger`, a
/// `pub(crate)` type of `roster`, one of its fields, two of its methods and a free function — the
/// `#carve` 21 R3 shape, where every one of them was widened by hand after the move.
pub fn a_workspace_whose_module_the_origin_still_reaches() -> AFixtureWorkspace {
    an_empty_fixture()
        .writing("Cargo.toml", THREE_CRATES)
        .writing("crates/shared/Cargo.toml", SHARED_MANIFEST)
        .writing("crates/shared/src/lib.rs", SHARED_LIB)
        .writing("crates/origin/Cargo.toml", ORIGIN_OVER_BOTH)
        .writing(
            ORIGIN_LIB,
            "//! The crate the roster leaves.\n\npub mod ledger;\npub mod roster;\npub mod runtime;\n",
        )
        .writing(THE_ROSTER, &source(ROSTER_BEFORE))
        .writing(
            THE_LEDGER,
            &source(&[
                "use crate::roster::AgentRoster;",
                "",
                "pub(crate) fn ledger_rev(roster: &AgentRoster) -> u64 {",
                "    roster.broadcast()",
                "}",
            ]),
        )
        .writing(
            "crates/origin/src/runtime.rs",
            &source(&[
                "use crate::ledger::ledger_rev;",
                "use crate::roster::{started_roster_rev, AgentRoster};",
                "",
                "pub fn boot(rev: u64) -> u64 {",
                "    let roster = AgentRoster::new();",
                "    roster.broadcast() + started_roster_rev(&roster) + ledger_rev(&roster) + roster.rev + rev",
                "}",
            ]),
        )
        .writing("crates/destination/Cargo.toml", DESTINATION_MANIFEST)
        .writing("crates/destination/src/lib.rs", "//! The crate the roster moves into.\n\n")
        .tracked_by_git()
}

/// The nested module [`a_workspace_whose_nested_module_its_parent_globs`] moves.
pub const THE_ATTACHMENT_PROGRESS: &str =
    "crates/origin/src/connection_service/attachment_progress.rs";
/// Where [`THE_ATTACHMENT_PROGRESS`] lands.
pub const THE_MOVED_ATTACHMENT_PROGRESS: &str = "crates/destination/src/attachment_progress.rs";
/// The parent that re-exports [`THE_ATTACHMENT_PROGRESS`] by glob.
pub const THE_CONNECTION_SERVICE: &str = "crates/origin/src/connection_service.rs";

/// A three-crate workspace whose `connection_service` declares a private child
/// `attachment_progress` and re-exports all of it with `pub(crate) use attachment_progress::*;` —
/// the `#carve` 15 shape of the 09-25 todo. Its sibling reaches the child's struct and field through
/// `use super::*;`, and calls a `pub(crate)` trait's method that no path names.
pub fn a_workspace_whose_nested_module_its_parent_globs() -> AFixtureWorkspace {
    an_empty_fixture()
        .writing("Cargo.toml", THREE_CRATES)
        .writing("crates/shared/Cargo.toml", SHARED_MANIFEST)
        .writing("crates/shared/src/lib.rs", SHARED_LIB)
        .writing("crates/origin/Cargo.toml", ORIGIN_OVER_BOTH)
        .writing(
            ORIGIN_LIB,
            "//! The crate the child leaves.\n\npub mod connection_service;\n",
        )
        .writing(
            THE_CONNECTION_SERVICE,
            &source(&[
                "//! The service.",
                "",
                "mod attachment_progress;",
                "pub(crate) use attachment_progress::*;",
                "",
                "pub mod sibling;",
            ]),
        )
        .writing(
            THE_ATTACHMENT_PROGRESS,
            &source(&[
                "pub(crate) struct AttachmentProgressSink {",
                "    pub(crate) sent: u64,",
                "}",
                "",
                "pub(crate) trait Progressing {",
                "    fn tick(&self) -> u64;",
                "}",
                "",
                "impl Progressing for AttachmentProgressSink {",
                "    fn tick(&self) -> u64 {",
                "        self.sent + private_helper()",
                "    }",
                "}",
                "",
                "fn private_helper() -> u64 {",
                "    0",
                "}",
            ]),
        )
        .writing(
            "crates/origin/src/connection_service/sibling.rs",
            &source(&[
                "use super::*;",
                "",
                "pub fn progress() -> u64 {",
                "    let sink = AttachmentProgressSink { sent: 1 };",
                "    sink.tick() + sink.sent",
                "}",
            ]),
        )
        .writing("crates/destination/Cargo.toml", DESTINATION_MANIFEST)
        .writing(
            "crates/destination/src/lib.rs",
            "//! The crate the child moves into.\n\n",
        )
        .tracked_by_git()
}

/// The module [`a_workspace_whose_moved_fn_returns_a_type_no_path_names`] moves.
pub const THE_FACTORY: &str = "crates/origin/src/factory.rs";
/// Where [`THE_FACTORY`] lands.
pub const THE_MOVED_FACTORY: &str = "crates/destination/src/factory.rs";

/// A three-crate workspace whose `runtime` calls `factory::make().size()`: `make` and `size` are
/// reached by path, while `Widget`, the type `make` returns, is named by nothing outside.
pub fn a_workspace_whose_moved_fn_returns_a_type_no_path_names() -> AFixtureWorkspace {
    an_empty_fixture()
        .writing("Cargo.toml", THREE_CRATES)
        .writing("crates/shared/Cargo.toml", SHARED_MANIFEST)
        .writing("crates/shared/src/lib.rs", SHARED_LIB)
        .writing("crates/origin/Cargo.toml", ORIGIN_OVER_BOTH)
        .writing(
            ORIGIN_LIB,
            "//! The crate the factory leaves.\n\npub mod factory;\npub mod runtime;\n",
        )
        .writing(
            THE_FACTORY,
            &source(&[
                "//! Makes widgets.",
                "",
                "pub(crate) struct Widget {",
                "    size: u64,",
                "}",
                "",
                "impl Widget {",
                "    pub(crate) fn size(&self) -> u64 {",
                "        self.size",
                "    }",
                "}",
                "",
                "pub(crate) fn make() -> Widget {",
                "    Widget { size: 3 }",
                "}",
            ]),
        )
        .writing(
            "crates/origin/src/runtime.rs",
            &source(&[
                "pub fn boot() -> u64 {",
                "    crate::factory::make().size()",
                "}",
            ]),
        )
        .writing("crates/destination/Cargo.toml", DESTINATION_MANIFEST)
        .writing(
            "crates/destination/src/lib.rs",
            "//! The crate the factory moves into.\n\n",
        )
        .tracked_by_git()
}

/// The `#carve` 21 R6 shape (`#reshape` 8/19): `origin`'s `connection_service` holds three members
/// that name each other through one grouped `use`, through the service's `pub use seed_codebase::*;`
/// glob, and through a `pub(in crate::connection_service)` function — the three shapes that were
/// hand-edited before the engine would move them.
pub fn a_service_whose_members_name_each_other_through_grouped_uses() -> AFixtureWorkspace {
    a_workspace_holding_files(&[
        ("Cargo.toml", THREE_CRATES),
        ("crates/shared/Cargo.toml", SHARED_MANIFEST),
        ("crates/shared/src/lib.rs", SHARED_LIB),
        ("crates/origin/Cargo.toml", ORIGIN_OVER_BOTH),
        ("crates/origin/src/lib.rs", "//! The origin.\n\npub mod connection_service;\n"),
        (
            "crates/origin/src/connection_service.rs",
            "//! The service.\n\npub mod agent_host_callbacks;\npub mod attached_initial_prompt;\n\
             pub mod seed_codebase;\npub mod seeded_clone_guard;\n\npub use seed_codebase::*;\n",
        ),
        (
            "crates/origin/src/connection_service/agent_host_callbacks.rs",
            "use crate::connection_service::attached_initial_prompt;\n\
             use crate::connection_service::{seed_codebase, seeded_clone_guard, SeededAgentClones};\n\n\
             pub fn callbacks(clones: &dyn SeededAgentClones) -> (u32, seeded_clone_guard::Guard) {\n    \
             (\n        seed_codebase::seed() + clones.count() + attached_initial_prompt::prompt(),\n        \
             seeded_clone_guard::Guard,\n    )\n}\n",
        ),
        (
            "crates/origin/src/connection_service/attached_initial_prompt.rs",
            "pub(in crate::connection_service) fn prompt() -> u32 {\n    2\n}\n",
        ),
        (
            "crates/origin/src/connection_service/seed_codebase.rs",
            "pub trait SeededAgentClones {\n    fn count(&self) -> u32;\n}\n\n\
             pub fn seed() -> u32 {\n    1\n}\n",
        ),
        (
            "crates/origin/src/connection_service/seeded_clone_guard.rs",
            "pub struct Guard;\n",
        ),
        ("crates/destination/Cargo.toml", DESTINATION_MANIFEST),
        ("crates/destination/src/lib.rs", "//! The destination.\n"),
    ])
}

/// One `move_cluster_to_crate` of `members` of `connection_service`, leaving a facade.
pub fn a_cluster_move_of_the_service_members(members: &[&str]) -> RefactorOp {
    let mut anchors = members.iter().map(|member| Anchor::Symbol {
        file: format!("crates/origin/src/connection_service/{member}.rs"),
        path: format!("connection_service::{member}"),
    });
    RefactorOp {
        anchor: anchors.next().expect("a cluster names at least one module"),
        also: anchors.collect(),
        ..a_cluster_move_of(&[members[0]], Some(Reexport::Glob))
    }
}
