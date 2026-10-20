//! A warm rust-analyzer index, served as `code_index.CodeIndexService`.
//!
//! This crate owns `proto/code_index.proto`, serves it, and publishes its coordinate. It exists
//! because `tddy-tools restructure` builds a `TaskRegistry` and an `LspRegistry` per invocation and
//! drops both on exit, so every run pays a full crate-graph load — six to ten minutes on a
//! workspace this size, once per retry of an iterative carve.
//!
//! The index is held per workspace root, in the shape `tddy_lsp::LspRegistry` already provides:
//! keyed by `(root, language)`, lazily started, reaped when idle, respawned when its task dies. One
//! process therefore serves several worktrees, and every request names the root it means.
//!
//! Two lifetimes, one implementation. With no transport argument the binary runs a single operation
//! against the generated service trait **in process** — prost structs in, prost structs out, no
//! encode/decode — and exits with a status. With `--grpc` and/or `--stdio` it serves that same
//! implementation and stays alive. There is one code path, not two that must be kept in step.

mod activity;
pub mod analyze;
mod apply;
mod graph;
pub mod index;
mod navigation;
pub mod operations;
mod plan_flush;
mod plan_upkeep;
pub mod queries;
pub mod service;
pub mod status;
mod symbols;
mod tree_changes;
mod warm;
mod watch_root;

pub use service::{
    build_code_index_entry, build_code_index_entry_with_supports, CodeIndexPorts,
    CodeIndexServiceImpl, EventStream,
};

pub mod proto {
    /// The canonical message types, the `tddy-rpc` service trait and server, and the tonic adapter
    /// that delegates one implementation of that trait to the gRPC server trait below.
    pub mod code_index {
        include!(concat!(env!("OUT_DIR"), "/code_index.rs"));
    }

    /// The tonic gRPC server and client, over the same message types via `extern_path`.
    pub mod tonic_code_index {
        #![allow(unused_imports, clippy::all)]
        include!(concat!(env!("OUT_DIR"), "/tonic_code_index/code_index.rs"));
    }
}

/// The coordinate a client addresses this service at.
///
/// It lives in this crate rather than in `tddy-service` because this crate owns
/// `code_index.proto`, is the only crate that serves it, and is a dependency of every caller that
/// addresses it — the rule `tddy-terminal-rpc` states for `TERMINAL_SESSION_SERVICE`. Only
/// coordinates whose served and addressed ends sit in different crates belong in the shared
/// catalog.
pub const CODE_INDEX_SERVICE: &str = "code_index.CodeIndexService";

/// The manifest whose presence at a root makes it a Rust workspace — the one kind of root this
/// index serves.
pub const RUST_WORKSPACE_MANIFEST: &str = "Cargo.toml";

/// Whether `root` is a Rust workspace this index can serve: it has a [`RUST_WORKSPACE_MANIFEST`]
/// at its top. Callers ask before warming a root or dialling the index daemon for it.
#[must_use]
pub fn is_rust_workspace(root: &std::path::Path) -> bool {
    root.join(RUST_WORKSPACE_MANIFEST).is_file()
}

/// Whether this index can serve `root`: a Rust workspace ([`is_rust_workspace`]), or a tree holding
/// a file one of `supports` claims whose support launches a server — an extension-only worktree
/// whose language has a configured server (`#reshape` extension-language-server, R7). The
/// predicate a caller asks in place of [`is_rust_workspace`] before warming a root or reporting it
/// has no index.
///
/// Walks the tree for a claimed file, skipping `.git` and `target`; a support with no server
/// (outline only) never makes a root servable.
#[must_use]
pub fn serves_workspace(
    root: &std::path::Path,
    supports: &tddy_code_restructuring::SupportRegistry,
) -> bool {
    let _ = (root, supports);
    // TODO(reshape-extension-language-server): implement
    todo!("serves_workspace")
}

/// The proto's file-descriptor set, so a host can merge this service into gRPC reflection without
/// re-parsing the schema.
pub static CODE_INDEX_DESCRIPTOR_BYTES: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/code_index_descriptors.bin"));

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use tddy_code_restructuring::{
        LanguageBackend, LanguageSupport, ServerHandle, SupportRegistry,
    };
    use tddy_lsp::{LanguageId, LaunchSpec};

    /// A support claiming files by suffix, launching `launch` — a stand-in for an installed
    /// extension's, so this predicate is pinned without loading a grammar.
    struct ASupport {
        id: LanguageId,
        suffix: &'static str,
        launch: Option<LaunchSpec>,
    }

    impl LanguageSupport for ASupport {
        fn id(&self) -> &LanguageId {
            &self.id
        }

        fn claims(&self, path: &Path) -> bool {
            path.to_string_lossy().ends_with(self.suffix)
        }

        fn launch(&self) -> Option<LaunchSpec> {
            self.launch.clone()
        }

        fn workspace_root(&self, _file: &Path, root: &Path) -> PathBuf {
            root.to_path_buf()
        }

        fn backend(&self, _server: ServerHandle<'_>, _root: &Path) -> Box<dyn LanguageBackend> {
            unreachable!("the predicate builds no backend")
        }
    }

    fn supports_of(support: ASupport) -> SupportRegistry {
        let mut supports = SupportRegistry::new();
        supports.register(Box::new(support));
        supports
    }

    fn a_toml_support_launching(launch: Option<LaunchSpec>) -> ASupport {
        ASupport {
            id: LanguageId::new("demo-toml"),
            suffix: ".toml",
            launch,
        }
    }

    /// A tree with no `Cargo.toml`, holding `config/app.toml` and a `target/` the walk skips.
    fn an_extension_only_tree() -> tempfile::TempDir {
        let tree = tempfile::tempdir().expect("a temporary tree");
        std::fs::create_dir_all(tree.path().join("config")).expect("config dir");
        std::fs::write(tree.path().join("config/app.toml"), "[server]\n").expect("a toml file");
        tree
    }

    #[test]
    fn a_tree_whose_files_a_server_launching_support_claims_is_served() {
        // Given an extension-only tree, and a support with a server that claims its TOML file
        let tree = an_extension_only_tree();
        let supports = supports_of(a_toml_support_launching(Some(LaunchSpec::new(
            "/opt/fake-ls",
        ))));

        // When the index is asked whether it serves the tree
        let served = (
            is_rust_workspace(tree.path()),
            serves_workspace(tree.path(), &supports),
        );

        // Then it is no Rust workspace, and served all the same
        assert_eq!(served, (false, true));
    }

    #[test]
    fn a_tree_claimed_only_by_a_support_without_a_server_is_not_served() {
        // Given an extension-only tree, and an outline-only support claiming its TOML file
        let tree = an_extension_only_tree();
        let supports = supports_of(a_toml_support_launching(None));

        // When the index is asked whether it serves the tree
        let served = serves_workspace(tree.path(), &supports);

        // Then it is not: there is no server to hold it
        assert!(!served, "an outline-only language made the tree servable");
    }

    #[test]
    fn a_rust_workspace_is_served_with_no_supports() {
        // Given a tree with a Cargo manifest, and no supports beyond Rust
        let tree = tempfile::tempdir().expect("a temporary tree");
        std::fs::write(tree.path().join(RUST_WORKSPACE_MANIFEST), "[workspace]\n")
            .expect("a manifest");

        // When the index is asked whether it serves the tree
        let served = serves_workspace(tree.path(), &SupportRegistry::new());

        // Then it is, as it always was
        assert!(served, "a Rust workspace stopped being served");
    }
}
