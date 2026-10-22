//! Test support for the language-server suite: the `demo-toml` fixture with a server declared for
//! its language, a `settings.json` beside it, and the server a support's launch starts — the
//! deterministic `fake_lsp` from `tddy-lsp`, never a real language server.

#![allow(
    dead_code,
    reason = "each test binary uses a different subset of the helpers"
)]

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tddy_code_restructuring::backends::rust::discard;
use tddy_code_restructuring::overlay::Overlay;
use tddy_code_restructuring::registry::Workspace;
use tddy_code_restructuring::spawn_record::SpawnRecorder;
use tddy_code_restructuring::{
    LanguageSupport, LiveServer, RefactorOp, Resolution, RestructureError, ServerHandle,
};
use tddy_lsp::{LspAllowList, LspKey, LspRegistry, LspService};
use tddy_task::TaskRegistry;
use tddy_zed_extensions::{load_installed, ExtensionSettings, ExtensionSupport};
use tokio_util::sync::CancellationToken;

use crate::outline_support::{AFixtureExtension, AnExtensionsDir};

/// The server id the suite declares for `Demo TOML`.
pub const FAKE_LS: &str = "fake-ls";

/// The deterministic fake server — what `lsp.fake-ls.binary.path` names.
pub fn the_fake_server() -> &'static str {
    env!("CARGO_BIN_EXE_fake_lsp")
}

impl AnExtensionsDir {
    /// Append `table` — `[language_servers.<id>]` entries — to the installed demo-toml manifest.
    pub fn declare_in_demo_toml(&self, table: &str) {
        let manifest = self
            .extensions
            .extension(AFixtureExtension::DemoToml.id())
            .join("extension.toml");
        let mut text = fs::read_to_string(&manifest).expect("read the installed manifest");
        text.push('\n');
        text.push_str(table);
        fs::write(&manifest, text).expect("write the installed manifest");
    }

    /// Declare `fake-ls` as the server of demo-toml's one language, `Demo TOML`.
    pub fn declare_fake_ls_for_demo_toml(&self) {
        self.declare_in_demo_toml("[language_servers.fake-ls]\nlanguages = [\"Demo TOML\"]\n");
    }

    /// Write `settings` as `<extensions dir>/settings.json`.
    pub fn write_settings(&self, settings: Value) {
        fs::write(
            ExtensionSettings::file(&self.extensions),
            settings.to_string(),
        )
        .expect("write the settings");
    }

    /// `<extensions dir>/settings.json`, as a refusal names it.
    pub fn settings_file(&self) -> String {
        ExtensionSettings::file(&self.extensions)
            .display()
            .to_string()
    }

    /// The one installed language's support, served as its settings say.
    pub fn the_demo_toml_support(&self) -> ExtensionSupport {
        let mut languages = load_installed(&self.extensions).expect("the extensions load");
        assert_eq!(
            languages.supports.len(),
            1,
            "expected demo-toml's one language"
        );
        languages.supports.remove(0)
    }
}

/// The server `support`'s launch starts for the workspace at `root`, through the registry a host
/// keeps its servers in.
pub async fn the_server_of(support: &ExtensionSupport, root: &Path) -> Arc<LspService> {
    let launch = support
        .launch()
        .expect("the support has a configured server");
    let mut allow = LspAllowList::new();
    allow.allow(support.id().clone(), launch);
    LspRegistry::new(allow, TaskRegistry::new(), Duration::from_secs(60))
        .get_or_spawn(LspKey {
            root: root.to_path_buf(),
            language: support.id().clone(),
        })
        .await
        .expect("the configured server starts")
}

/// Resolve `op` through `support`'s backend over `server` (none for the static tier) at `root`,
/// then apply what it resolved to — what a run does with one operation.
pub async fn resolved_and_applied(
    support: ExtensionSupport,
    server: Option<Arc<LspService>>,
    root: &Path,
    op: RefactorOp,
) -> Result<Resolution, RestructureError> {
    let root = root.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let cancel = CancellationToken::new();
        let progress = discard();
        let handle = match &server {
            Some(service) => ServerHandle::live(LiveServer {
                client: &service.client,
                cancel: &cancel,
                progress: &progress,
                trace: None,
                wait_heartbeat: Duration::from_secs(60),
            }),
            None => ServerHandle::none(),
        };
        let mut backend = support.backend(handle, &root);
        let overlay = Overlay::new();
        let resolution = backend.resolve(
            &op,
            &Workspace {
                root: &root,
                overlay: &overlay,
            },
        )?;
        tddy_code_restructuring::apply::apply_workspace_edit(
            &root,
            &resolution.edit,
            &SpawnRecorder::discard(),
        )?;
        Ok(resolution)
    })
    .await
    .expect("the blocking half of the run joins")
}

/// The Rust server a cold run starts beside an extension's — the same fake server, which a plan of
/// extension-language operations asks nothing.
pub async fn a_rust_server_for(root: &Path) -> Arc<LspService> {
    let mut allow = LspAllowList::new();
    allow.allow(
        tddy_lsp::LanguageId::RUST,
        tddy_lsp::LaunchSpec::new(the_fake_server()),
    );
    LspRegistry::new(allow, TaskRegistry::new(), Duration::from_secs(60))
        .get_or_spawn(LspKey {
            root: root.to_path_buf(),
            language: tddy_lsp::LanguageId::RUST,
        })
        .await
        .expect("the fake Rust server starts")
}

/// Commit everything under `root` as the baseline an apply starts from.
pub fn committed_as_the_baseline(root: &Path) {
    for args in [
        vec!["init", "--quiet"],
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
            .current_dir(root)
            .status()
            .expect("git runs");
        assert!(status.success(), "git {args:?} failed");
    }
}

/// Write `op` as a plan under `root` and apply it as a cold `restructure apply` does: `support`
/// installed, `server` started for its language and handed to the run beside the Rust client.
pub async fn applied_cold(
    support: ExtensionSupport,
    server: Arc<LspService>,
    root: &Path,
    op: &RefactorOp,
) -> Result<tddy_code_restructuring::runner::RunSummary, RestructureError> {
    let plan = root.join("rename.jsonl");
    fs::write(
        &plan,
        format!(
            "{{\"v\":1,\"snapshot\":{{}}}}\n{}\n",
            serde_json::to_string(op).expect("the operation serializes")
        ),
    )
    .expect("write the plan");
    let rust = a_rust_server_for(root).await;
    let language = support.id().clone();
    let mut installed = tddy_code_restructuring::SupportRegistry::new();
    installed.register(Box::new(support));
    let options = tddy_code_restructuring::runner::Options {
        target: Some(plan),
        installed: Arc::new(installed),
        installed_clients: vec![(language, Arc::clone(&server.client))],
        ..Default::default()
    };
    let root = root.to_path_buf();
    tokio::task::spawn_blocking(move || {
        tddy_code_restructuring::runner::apply(
            &root,
            options,
            Some(Arc::clone(&rust.client)),
            CancellationToken::new(),
        )
    })
    .await
    .expect("the blocking half of the run joins")
}
