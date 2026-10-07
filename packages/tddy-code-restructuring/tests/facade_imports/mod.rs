//! Fixtures and drivers shared by the `repoint_facade_imports` suites that need no rust-analyzer.
//!
//! The operation is text-only, so it is resolved here through a `RustBackend` over the fake language
//! server, which is never asked a question: a test that needs the server to answer would be testing
//! something this operation does not do.
//!
//! The workspace is four packages. `kernel` defines `config::{Settings, Limits, standard_limits}`
//! and `paths`; `agents` defines `roster`; `mid` re-exports `kernel::config` (a facade chain); `app`
//! depends on them, and its `lib.rs` is the facade under test.

#![allow(dead_code)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tddy_code_restructuring::apply::apply_workspace_edit;
use tddy_code_restructuring::backends::rust::{discard, RustBackend};
use tddy_code_restructuring::registry::{LanguageBackend, Workspace};
use tddy_code_restructuring::runner::{self, Command, Options};
use tddy_code_restructuring::spawn_record::SpawnRecorder;
use tddy_code_restructuring::{
    client_capabilities, server_settings, Overlay, RefactorOp, Resolution,
};
use tddy_lsp::{Language, LaunchSpec, LspAllowList, LspKey, LspRegistry};
use tddy_task::TaskRegistry;
use tokio_util::sync::CancellationToken;

use crate::harness::{a_workspace_holding_files, AFixtureWorkspace};

/// Every dependency `app` declares: the facade's targets, and a registry crate nobody can read.
pub const ALL_DEPENDENCIES: &str = "kernel = { path = \"../kernel\" }\n\
     agents = { path = \"../agents\" }\n\
     mid = { path = \"../mid\" }\n\
     regcrate = \"1\"\n";

/// What `app/src/lib.rs` holds: the facades, each forwarding to another crate or to a module of
/// this one.
pub const THE_FACADE: &str = concat!(
    "pub use agents::roster::*;\n",
    "pub use inner::Thing;\n",
    "pub use kernel::config;\n",
    "pub use kernel::config::Settings as AppSettings;\n",
    "pub use kernel::paths as user_paths;\n",
    "pub use mid::config as mid_config;\n",
    "pub use regcrate::Clock;\n",
    "\n",
    "pub mod mix {\n",
    "    pub use agents::roster::Roster;\n",
    "    pub use kernel::config::Limits;\n",
    "}\n",
    "\n",
    "mod a;\n",
    "mod b;\n",
    "mod inner;\n",
);

/// The workspace with `app` declaring `dependencies`, holding `a.rs` and the `extra` files
/// (paths relative to the `app` package) beside the facade.
pub fn an_app_declaring(
    dependencies: &str,
    dev_dependencies: &str,
    a: &str,
    extra: &[(&str, &str)],
) -> AFixtureWorkspace {
    let app_manifest = format!(
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
         [dependencies]\n{dependencies}\n[dev-dependencies]\n{dev_dependencies}"
    );
    let mut files: Vec<(String, String)> = vec![
        (
            "Cargo.toml".into(),
            "[workspace]\nresolver = \"2\"\nmembers = [\"app\", \"kernel\", \"agents\", \"mid\"]\n"
                .into(),
        ),
        ("app/Cargo.toml".into(), app_manifest),
        ("app/src/lib.rs".into(), THE_FACADE.into()),
        ("app/src/a.rs".into(), a.into()),
        ("app/src/b.rs".into(), "pub struct Thing;\n".into()),
        ("app/src/inner.rs".into(), "pub struct Thing;\n".into()),
        (
            "kernel/Cargo.toml".into(),
            "[package]\nname = \"kernel\"\nversion = \"0.1.0\"\nedition = \"2021\"\n".into(),
        ),
        (
            "kernel/src/lib.rs".into(),
            "pub mod config;\npub mod paths;\n".into(),
        ),
        (
            "kernel/src/config.rs".into(),
            concat!(
                "pub struct Settings {\n    pub verbose: bool,\n}\n\n",
                "pub struct Limits {\n    pub max: u32,\n}\n\n",
                "pub fn standard_limits() -> Limits {\n    Limits { max: 8 }\n}\n",
            )
            .into(),
        ),
        (
            "kernel/src/paths.rs".into(),
            "pub fn home() -> &'static str {\n    \"/home\"\n}\n".into(),
        ),
        (
            "agents/Cargo.toml".into(),
            "[package]\nname = \"agents\"\nversion = \"0.1.0\"\nedition = \"2021\"\n".into(),
        ),
        ("agents/src/lib.rs".into(), "pub mod roster;\n".into()),
        ("agents/src/roster.rs".into(), "pub struct Roster;\n".into()),
        (
            "mid/Cargo.toml".into(),
            "[package]\nname = \"mid\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [dependencies]\nkernel = { path = \"../kernel\" }\n"
                .into(),
        ),
        ("mid/src/lib.rs".into(), "pub use kernel::config;\n".into()),
    ];
    files.extend(
        extra
            .iter()
            .map(|(path, text)| (format!("app/{path}"), (*text).to_string())),
    );
    let borrowed: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    a_workspace_holding_files(&borrowed)
}

/// The workspace with `app` declaring every dependency, and `a.rs` holding `a`.
pub fn an_app_whose_a_rs_holds(a: &str) -> AFixtureWorkspace {
    an_app_declaring(ALL_DEPENDENCIES, "", a, &[])
}

/// `app/src/a.rs` of the workspace above.
pub const A_RS: &str = "app/src/a.rs";

/// A `repoint_facade_imports` over the one file `file`, anchored by symbol.
pub fn a_repoint_of_the_file(file: &str) -> RefactorOp {
    an_operation(serde_json::json!({
        "op": "repoint_facade_imports",
        "anchor": { "kind": "symbol", "file": file, "path": "app" },
    }))
}

/// A `repoint_facade_imports` over the module that the `mod <name>;` line of `parent_file` declares,
/// anchored the way an `items` anchor on that declaration is once it has been resolved: by the range
/// of the declaration.
pub fn a_repoint_of_the_module(
    workspace: &AFixtureWorkspace,
    parent_file: &str,
    name: &str,
) -> RefactorOp {
    let declaration = format!("mod {name};");
    let text = workspace.read(parent_file);
    let (index, line) = text
        .lines()
        .enumerate()
        .find(|(_, line)| line.trim_end().ends_with(&declaration))
        .expect("the parent file declares the module");
    let first = line.len() - declaration.len() + 1;
    an_operation(serde_json::json!({
        "op": "repoint_facade_imports",
        "anchor": {
            "kind": "range",
            "file": parent_file,
            "start": { "line": index + 1, "col": first },
            "end": { "line": index + 1, "col": line.len() + 1 },
        },
    }))
}

/// A plan operation read from its JSON, the way a plan line is.
pub fn an_operation(json: serde_json::Value) -> RefactorOp {
    serde_json::from_value(json).expect("the operation reads")
}

/// The deterministic fake language server, which this operation must never need to ask anything.
async fn a_server_nobody_asks(root: &std::path::Path) -> Arc<tddy_lsp::client::LspClient> {
    let mut allow = LspAllowList::new();
    allow.allow(
        Language::Rust,
        LaunchSpec::new(env!("CARGO_BIN_EXE_fake_lsp"))
            .with_capabilities(client_capabilities())
            .with_initialization_options(server_settings()),
    );
    let registry = LspRegistry::new(allow, TaskRegistry::new(), Duration::from_secs(60));
    let service = registry
        .get_or_spawn(LspKey {
            root: root.to_path_buf(),
            language: Language::Rust,
        })
        .await
        .expect("the fake language server starts");
    Arc::clone(&service.client)
}

/// What resolving `op` against the files on disk gives, or the refusal it fails with.
pub async fn resolving(
    workspace: &AFixtureWorkspace,
    op: RefactorOp,
) -> Result<Resolution, String> {
    resolving_through(workspace, op, Overlay::new()).await
}

/// [`resolving`], reading through `overlay` first, as the operation after an earlier one does.
pub async fn resolving_through(
    workspace: &AFixtureWorkspace,
    op: RefactorOp,
    overlay: Overlay,
) -> Result<Resolution, String> {
    let root = workspace.path().to_path_buf();
    let client = a_server_nobody_asks(&root).await;
    tokio::task::spawn_blocking(move || {
        let mut backend = RustBackend::from_lsp_client(client, None, discard());
        backend
            .resolve(
                &op,
                &Workspace {
                    root: &root,
                    overlay: &overlay,
                },
            )
            .map_err(|error| error.to_string())
    })
    .await
    .expect("the blocking half of the resolution joins")
}

/// What the static `check` of the backend finds in `op`, without any server.
pub async fn statically_checking(
    workspace: &AFixtureWorkspace,
    op: RefactorOp,
) -> Result<Vec<String>, String> {
    let root = workspace.path().to_path_buf();
    let client = a_server_nobody_asks(&root).await;
    tokio::task::spawn_blocking(move || {
        let mut backend = RustBackend::from_lsp_client(client, None, discard());
        backend
            .check(
                &op,
                &Workspace {
                    root: &root,
                    overlay: &Overlay::new(),
                },
            )
            .map_err(|error| error.to_string())
    })
    .await
    .expect("the blocking half of the check joins")
}

/// Resolve `op`, write its edit, and return what `file` holds afterwards.
pub async fn the_file_after(workspace: &AFixtureWorkspace, op: RefactorOp, file: &str) -> String {
    let resolved = resolving(workspace, op)
        .await
        .expect("the operation resolves");
    apply_workspace_edit(workspace.path(), &resolved.edit, &SpawnRecorder::discard())
        .expect("the edit applies");
    workspace.read(file)
}

/// The refusal `op` is met with, or a failure saying it was not refused.
pub async fn refusal_of(workspace: &AFixtureWorkspace, op: RefactorOp) -> String {
    match resolving(workspace, op).await {
        Err(refusal) => refusal,
        Ok(resolved) => panic!("the operation was expected to be refused, and gave {resolved:?}"),
    }
}

/// A deep check of the plan at `plan` through the fake server, which the operations under test
/// never ask anything: the findings it gave, and every line of its account.
pub async fn deep_checking(
    workspace: &AFixtureWorkspace,
    plan: std::path::PathBuf,
) -> (Result<Vec<String>, String>, Vec<String>) {
    let root = workspace.path().to_path_buf();
    let client = a_server_nobody_asks(&root).await;
    let heard: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let keeper = Arc::clone(&heard);
    let options = Options {
        command: Command::Check,
        target: Some(plan),
        deep: true,
        account: Arc::new(move |line: &str| {
            keeper.lock().expect("the account").push(line.to_string());
        }),
        ..Options::default()
    };
    let found = tokio::task::spawn_blocking(move || {
        runner::check(&root, options, Some(client), CancellationToken::new())
    })
    .await
    .expect("the blocking half of the check joins")
    .map(|findings| findings.into_iter().map(|finding| finding.detail).collect())
    .map_err(|error| error.to_string());
    let account = heard.lock().expect("the account").clone();
    (found, account)
}

/// The lines of an account that are notes, in the order they were said.
pub fn the_notes_of(account: &[String]) -> Vec<String> {
    account
        .iter()
        .filter(|line| line.starts_with("   note: "))
        .cloned()
        .collect()
}
