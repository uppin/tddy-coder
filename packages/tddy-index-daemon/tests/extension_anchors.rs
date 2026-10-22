//! The index daemon answers an installed extension language's anchors as the cold CLI does
//! (`#reshape` extension-outline, PRD: "the same anchors come back through the index daemon as
//! through the cold CLI").
//!
//! The host is built as production builds it — the built-ins, then the installed extensions, the
//! allow-list from that registry, the entry through [`build_code_index_entry_with_supports`] — over
//! a data dir whose extensions dir holds `tddy-zed-extensions`' `demo-toml` fixture (the vendored
//! tree-sitter-toml grammar) in Zed's installed layout. The rust-analyzer the built-in Rust support
//! launches does not exist: a file an extension language claims must need no language server.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use prost::Message;
use tddy_code_restructuring::backends::rust::RustSupport;
use tddy_code_restructuring::{Anchor, Fingerprint, ItemPath, SupportRegistry};
use tddy_index_daemon::proto::code_index::{AnchorsRequest, AnchorsResponse};
use tddy_index_daemon::{build_code_index_entry_with_supports, CodeIndexPorts};
use tddy_lsp::LspRegistry;
use tddy_task::TaskRegistry;
use tddy_zed_extensions::{ExtensionsDir, InstalledExtensions};

const APP: &str = "config/app.toml";
const APP_TOML: &str = "[server]\nhost = \"localhost\"\nport = 8080\n\n[database]\nurl = \"postgres://localhost/app\"\npool = 4\n\n[logging]\nlevel = \"info\"\n";

/// A rust-analyzer that is not there: any request that needed a server would fail to start it.
const NO_RUST_ANALYZER: &str = "/nonexistent/rust-analyzer";

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

/// A tddy data dir with `demo-toml` installed: the fixture extension plus the vendored grammar as
/// its `grammars/toml.wasm`.
fn a_data_dir_with_demo_toml_installed() -> tempfile::TempDir {
    let data = tempfile::tempdir().expect("a temporary data dir");
    let installed = data.path().join("extensions/installed/demo-toml");
    copy_tree(&the_extension_fixtures().join("demo-toml"), &installed);
    std::fs::create_dir_all(installed.join("grammars")).expect("grammars dir");
    std::fs::copy(
        the_extension_fixtures().join("grammars/tree-sitter-toml/toml.wasm"),
        installed.join("grammars/toml.wasm"),
    )
    .expect("copy the vendored grammar");
    data
}

/// A workspace holding `config/app.toml` and no Cargo package.
fn a_workspace_holding_the_app_config() -> tempfile::TempDir {
    let workspace = tempfile::tempdir().expect("a temporary workspace");
    std::fs::create_dir_all(workspace.path().join("config")).expect("config dir");
    std::fs::write(workspace.path().join(APP), APP_TOML).expect("write the app config");
    workspace
}

/// The built-ins — Rust launching [`NO_RUST_ANALYZER`] — then the extensions installed under
/// `data`: the registry a host builds.
fn the_supports_installed_under(data: &Path) -> SupportRegistry {
    let mut supports = SupportRegistry::with_builtin_rust(RustSupport::launching(NO_RUST_ANALYZER));
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

/// Dispatch `Anchors` at the registered coordinate and decode its answer.
async fn the_anchor_asked_of(
    entry: &tddy_rpc::ServiceEntry,
    request: AnchorsRequest,
) -> Result<AnchorsResponse, tddy_rpc::Status> {
    let message = tddy_rpc::RpcMessage::new(
        request.encode_to_vec(),
        tddy_rpc::RequestMetadata::over(tddy_rpc::RequestTransport::Direct),
    );
    match entry
        .service
        .handle_rpc(entry.name, "Anchors", &message)
        .await
    {
        tddy_rpc::RpcResult::Unary(Ok(payload)) => {
            Ok(AnchorsResponse::decode(payload.as_slice()).expect("the answer decodes"))
        }
        tddy_rpc::RpcResult::Unary(Err(status)) => Err(status),
        tddy_rpc::RpcResult::ServerStream(_) => panic!("expected a unary answer from Anchors"),
    }
}

/// Lines `first..=last` of the app config, fingerprinted as every item anchor is.
fn the_fingerprint_of_lines(first: usize, last: usize) -> Fingerprint {
    let lines: Vec<&str> = APP_TOML.split('\n').collect();
    Fingerprint::of(&lines[first - 1..last].join("\n"))
}

/// The anchor the cold CLI prints for `anchors config/app.toml --items server,database` — the one
/// `tddy-zed-extensions`' outline suite pins at library level, rendered by the console both front
/// ends share.
fn the_anchor_the_cli_prints() -> String {
    tddy_code_restructuring::console::item_anchor(&Anchor::Items {
        file: APP.to_string(),
        items: ["server", "database"]
            .map(|name| ItemPath::parse(&format!("{APP}::{name}")).expect("an item path"))
            .to_vec(),
        fingerprints: vec![
            the_fingerprint_of_lines(1, 3),
            the_fingerprint_of_lines(5, 7),
        ],
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn the_daemon_answers_the_same_anchors_as_the_cli() {
    // Given a workspace with a TOML file, and a host with the demo-toml extension installed
    let data = a_data_dir_with_demo_toml_installed();
    let workspace = a_workspace_holding_the_app_config();
    let entry = a_host_over(the_supports_installed_under(data.path()));

    // When the daemon is asked for an anchor over two adjacent tables
    let answer = the_anchor_asked_of(
        &entry,
        AnchorsRequest {
            workspace_root: workspace.path().to_string_lossy().into_owned(),
            file: APP.to_string(),
            items: vec!["server".to_string(), "database".to_string()],
            at: None,
        },
    )
    .await
    .expect("an extension-language file is anchored without a language server");

    // Then it answers the anchor the cold CLI prints
    assert_eq!(answer.anchor_json, the_anchor_the_cli_prints());
}
