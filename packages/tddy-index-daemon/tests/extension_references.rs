//! The index daemon answers an installed extension language's code-index queries, and applies its
//! renames, through the server its settings configure (`#reshape` extension-language-server, R6–R7;
//! PRD: "the index daemon answers references for an extension-language file").
//!
//! The host is built as production builds it — the built-ins, then the installed extensions, the
//! allow-list from that registry — over a data dir holding `tddy-zed-extensions`' `demo-toml`
//! fixture with `[language_servers.fake-ls]` declared for its language and
//! `<extensions dir>/settings.json` pointing `lsp.fake-ls.binary.path` at `tddy-lsp`'s
//! deterministic `fake_lsp`. The built-in Rust support launches the same fake server, so no real
//! language server runs.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use pretty_assertions::assert_eq;
use prost::Message;
use serde_json::json;
use tddy_code_restructuring::backends::rust::RustSupport;
use tddy_code_restructuring::{Fingerprint, SupportRegistry};
use tddy_index_daemon::proto::code_index::{
    restructure_event, ApplyRequest, CodeLocation, ReferencesRequest, ReferencesResponse,
    RestructureEvent, SourcePosition, SourceRange,
};
use tddy_index_daemon::{
    build_code_index_entry_with_supports, is_rust_workspace, serves_workspace, CodeIndexPorts,
};
use tddy_lsp::LspRegistry;
use tddy_task::TaskRegistry;
use tddy_zed_extensions::{ExtensionsDir, InstalledExtensions};

const APP: &str = "config/app.toml";
const APP_TOML: &str = "[server]\nhost = \"localhost\"\nport = 8080\n\n[database]\nurl = \"postgres://localhost/app\"\npool = 4\n\n[logging]\nlevel = \"info\"\n";

fn the_extension_fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../tddy-zed-extensions/tests/fixtures")
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create the target");
    for entry in std::fs::read_dir(from).expect("read a fixture dir") {
        let entry = entry.expect("a fixture entry");
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy a fixture file");
        }
    }
}

/// A tddy data dir with `demo-toml` installed, `fake-ls` declared as its language's server, and
/// `settings.json` running the fake server — re-rooting its answers under the workspace it was
/// started for — as `fake-ls`.
fn a_data_dir_with_a_served_extension_language() -> tempfile::TempDir {
    a_data_dir_running_fake_ls_with(&["--answers-in-its-workspace"])
}

/// [`a_data_dir_with_a_served_extension_language`], the fake server started with `arguments`.
fn a_data_dir_running_fake_ls_with(arguments: &[&str]) -> tempfile::TempDir {
    let data = tempfile::tempdir().expect("a temporary data dir");
    let installed = data.path().join("extensions/installed/demo-toml");
    copy_tree(&the_extension_fixtures().join("demo-toml"), &installed);
    std::fs::create_dir_all(installed.join("grammars")).expect("grammars dir");
    std::fs::copy(
        the_extension_fixtures().join("grammars/tree-sitter-toml/toml.wasm"),
        installed.join("grammars/toml.wasm"),
    )
    .expect("copy the vendored grammar");
    let manifest = installed.join("extension.toml");
    let mut text = std::fs::read_to_string(&manifest).expect("read the manifest");
    text.push_str("\n[language_servers.fake-ls]\nlanguages = [\"Demo TOML\"]\n");
    std::fs::write(&manifest, text).expect("declare the server");
    let settings = json!({
        "lsp": { "fake-ls": { "binary": {
            "path": env!("CARGO_BIN_EXE_fake_lsp"),
            "arguments": arguments,
        } } }
    });
    std::fs::write(
        data.path().join("extensions/settings.json"),
        settings.to_string(),
    )
    .expect("write the settings");
    data
}

/// A git worktree holding `config/app.toml` and no Cargo package.
fn an_extension_only_worktree() -> tempfile::TempDir {
    let workspace = tempfile::tempdir().expect("a temporary workspace");
    std::process::Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(workspace.path())
        .status()
        .expect("git init");
    std::fs::create_dir_all(workspace.path().join("config")).expect("config dir");
    std::fs::write(workspace.path().join(APP), APP_TOML).expect("write the app config");
    workspace
}

/// The built-ins — Rust launching the fake server — then the extensions installed under `data`: the
/// registry a host builds.
fn the_supports_installed_under(data: &Path) -> SupportRegistry {
    let mut supports =
        SupportRegistry::with_builtin_rust(RustSupport::launching(env!("CARGO_BIN_EXE_fake_lsp")));
    let failures = supports
        .with_installed_extensions(&ExtensionsDir::under_data_dir(data))
        .expect("the extensions register");
    assert!(
        failures.is_empty(),
        "a grammar failed to load: {failures:?}"
    );
    supports
}

/// A host over `supports`, its servers allowed exactly as the registry launches them.
fn a_host_over(supports: SupportRegistry) -> tddy_rpc::ServiceEntry {
    let servers = LspRegistry::new(
        supports.allow_list(),
        TaskRegistry::new(),
        Duration::from_secs(60),
    );
    build_code_index_entry_with_supports(
        CodeIndexPorts {
            servers,
            wait_heartbeat: tddy_code_restructuring::backends::rust::WAIT_HEARTBEAT,
        },
        Arc::new(supports),
    )
}

/// Commit everything in `workspace` as its baseline, as a tree an apply runs over is.
fn committed(workspace: tempfile::TempDir) -> tempfile::TempDir {
    for args in [
        vec!["add", "--all"],
        vec![
            "-c",
            "user.name=tddy tests",
            "-c",
            "user.email=tests@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "--message",
            "baseline",
        ],
    ] {
        let status = std::process::Command::new("git")
            .args(&args)
            .current_dir(workspace.path())
            .status()
            .expect("git runs");
        assert!(status.success(), "git {args:?} failed");
    }
    workspace
}

/// A plan in `workspace` renaming `port` under `[server]` — line 3 of [`APP_TOML`] — to `new_name`,
/// anchored by item as `restructure anchors` writes it.
fn a_plan_renaming_the_port_key_to(workspace: &Path, new_name: &str) -> PathBuf {
    let port_line = APP_TOML
        .split('\n')
        .nth(2)
        .expect("line 3 of the app config");
    let op = json!({
        "id": "r1",
        "op": "rename_symbol",
        "anchor": {
            "kind": "item",
            "file": APP,
            "item": format!("{APP}::server::port"),
            "fingerprint": Fingerprint::of(port_line),
        },
        "name": new_name,
    });
    let plan = workspace.join("rename.jsonl");
    std::fs::write(&plan, format!("{{\"v\":1,\"snapshot\":{{}}}}\n{op}\n"))
        .expect("write the plan");
    plan
}

/// Dispatch `Apply` for `plan` and collect every event it streams, refusals included.
async fn the_events_of_applying(
    entry: &tddy_rpc::ServiceEntry,
    root: &Path,
    plan: &Path,
) -> Vec<Result<RestructureEvent, tddy_rpc::Status>> {
    let request = ApplyRequest {
        workspace_root: root.to_string_lossy().into_owned(),
        plan: plan.to_string_lossy().into_owned(),
        dry_run: false,
        resume: false,
        from: None,
        stop_after: None,
    };
    let message = tddy_rpc::RpcMessage::new(
        request.encode_to_vec(),
        tddy_rpc::RequestMetadata::over(tddy_rpc::RequestTransport::Direct),
    );
    let tddy_rpc::RpcResult::ServerStream(Ok(mut receiver)) = entry
        .service
        .handle_rpc(entry.name, "Apply", &message)
        .await
    else {
        panic!("expected a stream of events from Apply");
    };
    let mut events = Vec::new();
    while let Some(next) = receiver.recv().await {
        events.push(next.map(|payload| {
            RestructureEvent::decode(payload.as_slice()).expect("the event decodes")
        }));
    }
    events
}

/// Dispatch `References` at the registered coordinate and decode its answer.
async fn the_references_asked_of(
    entry: &tddy_rpc::ServiceEntry,
    request: ReferencesRequest,
) -> Result<ReferencesResponse, tddy_rpc::Status> {
    let message = tddy_rpc::RpcMessage::new(
        request.encode_to_vec(),
        tddy_rpc::RequestMetadata::over(tddy_rpc::RequestTransport::Direct),
    );
    match entry
        .service
        .handle_rpc(entry.name, "References", &message)
        .await
    {
        tddy_rpc::RpcResult::Unary(Ok(payload)) => {
            Ok(ReferencesResponse::decode(payload.as_slice()).expect("the answer decodes"))
        }
        tddy_rpc::RpcResult::Unary(Err(status)) => Err(status),
        tddy_rpc::RpcResult::ServerStream(_) => panic!("expected a unary answer from References"),
    }
}

/// A location the fake server reports, relative to the workspace: one line, columns one-based.
fn a_location_in_the_workspace(file: &str, line: u32, from: u32, to: u32) -> CodeLocation {
    CodeLocation {
        file: file.to_string(),
        range: Some(SourceRange {
            start: Some(SourcePosition { line, column: from }),
            end: Some(SourcePosition { line, column: to }),
        }),
        outside_root: false,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_daemon_answers_references_for_an_extension_file() {
    // Given an extension-only worktree, and a host whose demo-toml language has a configured server
    let data = a_data_dir_with_a_served_extension_language();
    let workspace = an_extension_only_worktree();
    let entry = a_host_over(the_supports_installed_under(data.path()));

    // When the references of `port` in the TOML file are asked for
    let answer = the_references_asked_of(
        &entry,
        ReferencesRequest {
            workspace_root: workspace.path().to_string_lossy().into_owned(),
            file: APP.to_string(),
            position: Some(SourcePosition { line: 3, column: 1 }),
        },
    )
    .await
    .expect("an extension-language file is answered, not refused as not `.rs`");

    // Then the extension language's server answered, each location relative to the root
    assert_eq!(
        answer.locations,
        vec![
            a_location_in_the_workspace("src/lib.rs", 11, 1, 4),
            a_location_in_the_workspace("src/main.rs", 21, 5, 8),
        ]
    );
}

#[test]
fn an_extension_only_worktree_has_an_index_state() {
    // Given an extension-only worktree, and the demo-toml language with a configured server
    let data = a_data_dir_with_a_served_extension_language();
    let workspace = an_extension_only_worktree();
    let supports = the_supports_installed_under(data.path());

    // When the index is asked whether it serves the worktree
    let served = (
        is_rust_workspace(workspace.path()),
        serves_workspace(workspace.path(), Some(&supports)),
    );

    // Then it is no Rust workspace, and served all the same — it gets an index, not `NONE`
    assert_eq!(served, (false, true));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_daemon_renames_an_extension_symbol_through_its_server() {
    // Given an extension-only worktree, and a host whose demo-toml language has a configured server
    // that answers a rename with an edit of the identifier under the position
    let data = a_data_dir_running_fake_ls_with(&["--answers-rename"]);
    let workspace = committed(an_extension_only_worktree());
    let plan = a_plan_renaming_the_port_key_to(workspace.path(), "listen_port");
    let entry = a_host_over(the_supports_installed_under(data.path()));

    // When the plan is applied through the daemon
    let events = the_events_of_applying(&entry, workspace.path(), &plan).await;

    // Then the run applied the rename, and the server's edit is on disk: the key renamed where the
    // outline names it, nothing else touched (R6)
    let refusals: Vec<_> = events
        .iter()
        .filter_map(|event| event.as_ref().err())
        .collect();
    assert!(refusals.is_empty(), "the apply was refused: {refusals:?}");
    let applied = events
        .iter()
        .filter_map(|event| event.as_ref().ok()?.event.as_ref())
        .filter(|event| matches!(event, restructure_event::Event::Operation(_)))
        .count();
    assert_eq!(applied, 1, "events: {events:?}");
    assert_eq!(
        std::fs::read_to_string(workspace.path().join(APP)).expect("read the app config"),
        APP_TOML.replace("port = 8080", "listen_port = 8080")
    );
}
