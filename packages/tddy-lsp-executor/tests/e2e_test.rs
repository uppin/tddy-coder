//! End-to-end: drive the real `TddyLspExecutor` (target discovery → registry → LSP client)
//! against the deterministic `fake_lsp` server, over a two-target Rust workspace. Proves the
//! headline contract: two targets in one workspace share a single reused server, and queries
//! resolve through it.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tddy_core::toolcall::lsp::{LspExecutor, LspQuery};
use tddy_lsp::{Language, LaunchSpec, LspAllowList};
use tddy_lsp_executor::TddyLspExecutor;
use tddy_task::TaskRegistry;

static NEXT: AtomicU64 = AtomicU64::new(0);

/// A throwaway repo dir holding a `BUILD.yaml`, cleaned up on drop.
struct TempRepo {
    dir: PathBuf,
}

impl TempRepo {
    fn with_build_yaml(body: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "tddy-lsp-executor-e2e-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("BUILD.yaml"), body).unwrap();
        Self { dir }
    }

    fn path(&self) -> &Path {
        &self.dir
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn query(target: &str, file: &str) -> LspQuery {
    LspQuery {
        target: target.to_string(),
        file: file.to_string(),
        line: 10,
        character: 0,
        symbol_query: None,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn two_targets_in_one_workspace_share_one_server_and_resolve_queries() {
    // Given a workspace with two Rust targets and an executor over the fake server
    let repo = TempRepo::with_build_yaml(
        "schema_version: 1\ntargets:\n  - id: \"app:bin\"\n    name: bin\n    config:\n      type: rust_binary\n  - id: \"app:lib\"\n    name: lib\n    config:\n      type: rust_library\n",
    );
    let repo_path = repo.path().to_path_buf();
    let mut allow = LspAllowList::new();
    allow.allow(
        Language::Rust,
        LaunchSpec::new(env!("CARGO_BIN_EXE_fake_lsp")),
    );
    let tasks = TaskRegistry::new();
    let executor = Arc::new(TddyLspExecutor::new(
        allow,
        tasks.clone(),
        Duration::from_secs(60),
    ));

    // When one target asks for references and the other for a definition
    // (executor methods are sync + block on an internal runtime, so run them off-executor)
    let refs = {
        let (ex, rp) = (Arc::clone(&executor), repo_path.clone());
        tokio::task::spawn_blocking(move || ex.references(&rp, &query("app:bin", "src/main.rs")))
            .await
            .unwrap()
            .expect("references")
    };
    let defs = {
        let (ex, rp) = (Arc::clone(&executor), repo_path.clone());
        tokio::task::spawn_blocking(move || ex.definition(&rp, &query("app:lib", "src/lib.rs")))
            .await
            .unwrap()
            .expect("definition")
    };

    // Then both targets were served by a single, shared language-server task
    assert_eq!(
        tasks.list().await.len(),
        1,
        "expected one reused server task"
    );
    // And the queries resolved through it (fake returns two refs, one definition)
    assert_eq!(refs["references"].as_array().unwrap().len(), 2);
    assert_eq!(defs["locations"].as_array().unwrap().len(), 1);
}
