//! What a language brings to tddy, in one place.
//!
//! A [`LanguageSupport`] bundles the four facts that used to be wired in separately for Rust: which
//! files the language claims, how its server starts, how that server's workspace root is found from
//! a file, and the [`LanguageBackend`] that resolves its operations. A [`SupportRegistry`] holds the
//! supports and is the one place both the server allow-list and the backend registry are built from,
//! so a cold CLI run and the warm index daemon cannot drift apart on how a language is launched.
//!
//! Built-in supports are registered first and the first support that claims a file wins, so a
//! support registered later — an installed extension — never takes a file a built-in claims.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tddy_lsp::client::LspClient;
use tddy_lsp::{LanguageId, LaunchSpec, LspAllowList, LspError};
use tokio_util::sync::CancellationToken;

use crate::backends::rust::{ProgressSink, RustSupport};
use crate::registry::{BackendRegistry, LanguageBackend};

/// A language, as tddy knows it: its files, its server, its workspace root and its backend.
pub trait LanguageSupport: Send + Sync {
    /// The language's id; also the key its server is allowed and reused under.
    fn id(&self) -> &LanguageId;

    /// Whether `path` is a file of this language.
    fn claims(&self, path: &Path) -> bool;

    /// How the language's server starts, or `None` for a support that runs no server.
    fn launch(&self) -> Option<LaunchSpec>;

    /// The root the language's server is started at for `file`, searched no further out than
    /// `root`. A relative `file` is read against `root`.
    ///
    /// A file outside `root`, or one no root can be found for, is an error: no support picks a root
    /// for a file it cannot place.
    fn workspace_root(&self, file: &Path, root: &Path) -> Result<PathBuf, LspError>;

    /// The backend that resolves this language's operations over `server`, for the workspace at
    /// `root`.
    ///
    /// A support whose server already carries its root may leave `root` unused: the built-in Rust
    /// backend takes it from its live client, or from the operation that starts its server.
    fn backend(&self, server: ServerHandle<'_>, root: &Path) -> Box<dyn LanguageBackend>;
}

/// The running server a backend is built over, or none for the static tier.
#[derive(Clone, Copy)]
pub struct ServerHandle<'a> {
    live: Option<LiveServer<'a>>,
}

/// What a backend over a running server takes from its caller.
#[derive(Clone, Copy)]
pub struct LiveServer<'a> {
    /// The shared client of the server already started for this language.
    pub client: &'a Arc<LspClient>,
    /// What ends a wait for an index that is still loading.
    pub cancel: &'a CancellationToken,
    /// Where the server's own indexing lines go.
    pub progress: &'a ProgressSink,
    /// Where the diagnostic account of a seam goes, when the caller asked for one.
    pub trace: Option<fn(&str)>,
    /// The cadence at which a wait says what it is waiting for.
    pub wait_heartbeat: Duration,
}

impl<'a> ServerHandle<'a> {
    /// No server: a backend built over this answers only what text alone can tell.
    pub fn none() -> Self {
        Self { live: None }
    }

    /// The running server `server` describes.
    pub fn live(server: LiveServer<'a>) -> Self {
        Self { live: Some(server) }
    }

    /// The running server, if there is one.
    pub fn server(&self) -> Option<&LiveServer<'a>> {
        self.live.as_ref()
    }
}

/// The supports tddy knows, in the order they were registered.
#[derive(Default)]
pub struct SupportRegistry {
    supports: Vec<Box<dyn LanguageSupport>>,
}

impl SupportRegistry {
    /// No support at all.
    pub fn new() -> Self {
        Self::default()
    }

    /// The built-in supports, Rust launching `rust-analyzer` from `PATH`.
    pub fn with_builtins() -> Self {
        Self::with_builtin_rust(RustSupport::new())
    }

    /// The built-in supports with Rust as `rust` says — the index daemon launches the
    /// rust-analyzer it was configured with.
    pub fn with_builtin_rust(rust: RustSupport) -> Self {
        let mut registry = Self::new();
        registry.register(Box::new(rust));
        registry
    }

    /// Add `support` after every support already registered.
    pub fn register(&mut self, support: Box<dyn LanguageSupport>) -> &mut Self {
        self.supports.push(support);
        self
    }

    /// The first registered support that claims `path`.
    pub fn support_of(&self, path: &Path) -> Option<&dyn LanguageSupport> {
        self.supports
            .iter()
            .find(|support| support.claims(path))
            .map(|support| support.as_ref())
    }

    /// Every support's launch, allowed under its id; a support with no server is not allowed one.
    ///
    /// The first support registered under an id is the one whose launch is allowed, the same
    /// first-wins rule [`support_of`](Self::support_of) applies to files: a support registered later
    /// under a built-in's id never replaces the built-in's launch.
    pub fn allow_list(&self) -> LspAllowList {
        let mut allow = LspAllowList::new();
        for support in &self.supports {
            if allow.is_allowed(support.id()) {
                continue;
            }
            if let Some(launch) = support.launch() {
                allow.allow(support.id().clone(), launch);
            }
        }
        allow
    }

    /// Each support's backend over `server` for the workspace at `root`, in registration order.
    pub fn backends(&self, server: ServerHandle<'_>, root: &Path) -> BackendRegistry {
        let mut registry = BackendRegistry::new();
        for support in &self.supports {
            registry.register(support.backend(server, root));
        }
        registry
    }

    /// Each support's backend with no server for the workspace at `root`, for checks that read
    /// text alone.
    pub fn backends_static(&self, root: &Path) -> BackendRegistry {
        self.backends(ServerHandle::none(), root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A support that claims files by their suffix and launches whatever it is given.
    struct ASupport {
        id: LanguageId,
        suffixes: Vec<&'static str>,
        launch: Option<LaunchSpec>,
    }

    impl ASupport {
        fn named(id: &str) -> Self {
            Self {
                id: LanguageId::new(id),
                suffixes: Vec::new(),
                launch: None,
            }
        }

        fn claiming(mut self, suffix: &'static str) -> Self {
            self.suffixes.push(suffix);
            self
        }

        fn launching(mut self, spec: LaunchSpec) -> Self {
            self.launch = Some(spec);
            self
        }
    }

    impl LanguageSupport for ASupport {
        fn id(&self) -> &LanguageId {
            &self.id
        }

        fn claims(&self, path: &Path) -> bool {
            let name = path.to_string_lossy();
            self.suffixes.iter().any(|suffix| name.ends_with(suffix))
        }

        fn launch(&self) -> Option<LaunchSpec> {
            self.launch.clone()
        }

        fn workspace_root(&self, _file: &Path, root: &Path) -> Result<PathBuf, LspError> {
            Ok(root.to_path_buf())
        }

        fn backend(&self, _server: ServerHandle<'_>, _root: &Path) -> Box<dyn LanguageBackend> {
            unreachable!("no registry test builds a backend")
        }
    }

    fn a_registry_of(supports: Vec<ASupport>) -> SupportRegistry {
        let mut registry = SupportRegistry::new();
        for support in supports {
            registry.register(Box::new(support));
        }
        registry
    }

    fn the_language_of(registry: &SupportRegistry, path: &str) -> Option<String> {
        registry
            .support_of(Path::new(path))
            .map(|support| support.id().to_string())
    }

    #[test]
    fn claims_route_a_file_to_the_first_support_that_takes_it() {
        // Given two supports that both claim `.toml`, and one that claims `.md`
        let registry = a_registry_of(vec![
            ASupport::named("toml").claiming(".toml"),
            ASupport::named("cargo-manifest").claiming("Cargo.toml"),
            ASupport::named("markdown").claiming(".md"),
        ]);

        // When files are routed
        let routed = [
            the_language_of(&registry, "crates/a/Cargo.toml"),
            the_language_of(&registry, "docs/README.md"),
            the_language_of(&registry, "styles/site.css"),
        ];

        // Then each goes to the first support that claims it, and an unclaimed file to none
        assert_eq!(
            routed,
            [Some("toml".to_string()), Some("markdown".to_string()), None]
        );
    }

    #[test]
    fn a_later_support_never_takes_a_suffix_a_builtin_claims() {
        // Given the built-ins, then a support registered after them that also claims `.rs`
        let mut registry = SupportRegistry::with_builtins();
        registry.register(Box::new(ASupport::named("rust-like").claiming(".rs")));

        // When a Rust file is routed
        let routed = the_language_of(&registry, "src/lib.rs");

        // Then the built-in keeps it
        assert_eq!(routed, Some(LanguageId::RUST.to_string()));
    }

    #[test]
    fn a_later_support_under_a_builtins_id_never_replaces_its_launch() {
        // Given the built-ins, then a support registered after them under Rust's own id
        let mut registry = SupportRegistry::with_builtins();
        registry.register(Box::new(
            ASupport {
                id: LanguageId::RUST,
                ..ASupport::named("rust")
            }
            .launching(LaunchSpec::new("an-impostor-server")),
        ));

        // When the allow-list is built
        let allow = registry.allow_list();

        // Then Rust still launches the built-in's server
        assert_eq!(
            allow
                .launch_spec(&LanguageId::RUST)
                .map(|spec| spec.program.as_str()),
            Some("rust-analyzer")
        );
    }

    #[test]
    fn the_allow_list_carries_each_supports_launch() {
        // Given a support with a server and one that runs none
        let taplo = LaunchSpec::new("taplo")
            .with_language_id("toml")
            .with_initialization_options(serde_json::json!({ "formatter": {} }));
        let registry = a_registry_of(vec![
            ASupport::named("toml")
                .claiming(".toml")
                .launching(taplo.clone()),
            ASupport::named("outline-only").claiming(".scm"),
        ]);

        // When the allow-list is built
        let allow = registry.allow_list();

        // Then the server's launch is allowed under its id, exactly as the support gave it, and
        // the support without a server is not allowed one
        assert_eq!(
            (
                allow.launch_spec(&LanguageId::new("toml")),
                allow.is_allowed(&LanguageId::new("outline-only")),
            ),
            (Some(&taplo), false)
        );
    }
}
