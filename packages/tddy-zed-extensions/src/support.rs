//! An installed extension's languages as language supports (K11's interface): each claims its
//! `path_suffixes`, runs no server, and answers item anchors from its outline.
//!
//! Item anchors for these languages go through the same item-anchor traits Rust's do —
//! [`ItemResolver`] and [`ItemAtResolver`], routed by the [`BackendRegistry`] — so
//! `restructure anchors --items` / `--at`, an `items` / `item` anchor in a plan, and its
//! fingerprint check need nothing language-specific above this module.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tddy_code_restructuring::item_anchor::{ItemAtResolver, ItemResolver, ResolvedItem};
use tddy_code_restructuring::registry::Workspace;
use tddy_code_restructuring::{
    ItemPath, LanguageBackend, LanguageSupport, Range, RefactorKind, RefactorOp, Resolution,
    ServerHandle, SupportRegistry,
};
use tddy_lsp::{LanguageId, LaunchSpec, LspError};
use tree_sitter::{Language, Query};

use crate::dir::ExtensionsDir;
use crate::error::ExtensionError;
use crate::grammar::{GrammarLoadError, GrammarStore};
use crate::language::ExtensionLanguage;
use crate::outline::Outline;

/// What loading the installed extensions found: a support per language whose grammar loaded, and
/// one report per grammar that did not (R8).
pub struct InstalledLanguages {
    /// Sorted by extension id, then language name.
    pub supports: Vec<ExtensionSupport>,
    /// One per grammar that failed, however many languages share it.
    pub failures: Vec<GrammarLoadError>,
}

/// Every language of every extension under `dir.installed()`, each `installed/<id>/` read from its
/// own `extension.toml` — the installed layout, not `index.json`, so a directory left by a failed
/// install (a `.<id>.partial`) is skipped by its leading dot and nothing here needs the index.
///
/// No `installed/` directory is no extensions. A grammar that does not load is a
/// [`GrammarLoadError`] in [`InstalledLanguages::failures`] and its languages claim nothing; it
/// never fails the other extensions (R8). An extension whose manifest or language config cannot
/// be read is that [`ExtensionError`].
pub fn load_installed(dir: &ExtensionsDir) -> Result<InstalledLanguages, ExtensionError> {
    let _ = dir;
    // TODO(reshape-extension-outline): implement
    todo!("load_installed")
}

/// [`load_installed`] registered into a [`SupportRegistry`], after whatever it already holds — the
/// built-ins first, so a built-in keeps a suffix an extension also claims (R2, K11 R4).
pub trait InstalledExtensions {
    /// Register every installed extension language whose grammar loads, and hand back the reports
    /// of those that did not, for the front end to show once.
    fn with_installed_extensions(
        &mut self,
        dir: &ExtensionsDir,
    ) -> Result<Vec<GrammarLoadError>, ExtensionError>;
}

impl InstalledExtensions for SupportRegistry {
    fn with_installed_extensions(
        &mut self,
        dir: &ExtensionsDir,
    ) -> Result<Vec<GrammarLoadError>, ExtensionError> {
        let _ = dir;
        // TODO(reshape-extension-outline): implement
        todo!("SupportRegistry::with_installed_extensions")
    }
}

/// One installed extension language as a [`LanguageSupport`].
#[allow(
    dead_code,
    reason = "TODO(reshape-extension-outline): grammar, outline and store are read once implemented"
)]
pub struct ExtensionSupport {
    id: LanguageId,
    language: ExtensionLanguage,
    grammar: Language,
    /// `outline.scm` compiled against `grammar`; `None` when the language ships none.
    outline: Option<Arc<Query>>,
    /// The process's one store, shared by every extension language.
    store: Arc<Mutex<GrammarStore>>,
}

impl ExtensionSupport {
    /// The support for `language`, its grammar loaded into `store` and its `outline.scm` compiled.
    /// A grammar that does not load, or an `outline.scm` that does not compile against it, is the
    /// [`GrammarLoadError`] R8 reports.
    pub fn new(
        language: ExtensionLanguage,
        store: Arc<Mutex<GrammarStore>>,
    ) -> Result<Self, GrammarLoadError> {
        let _ = (language, store);
        // TODO(reshape-extension-outline): implement
        todo!("ExtensionSupport::new")
    }

    /// The language this support was built from.
    pub fn language(&self) -> &ExtensionLanguage {
        &self.language
    }

    /// The outline of `text`, a file of this language (R3).
    pub fn outline_of(&self, text: &str) -> Outline {
        let _ = text;
        // TODO(reshape-extension-outline): implement
        todo!("ExtensionSupport::outline_of")
    }

    /// The outline backend over this language, reading its files under `root` — the tree item
    /// anchors' files are relative to.
    pub fn outline_backend(&self, root: &Path) -> OutlineBackend {
        let _ = root;
        // TODO(reshape-extension-outline): implement
        todo!("ExtensionSupport::outline_backend")
    }
}

impl LanguageSupport for ExtensionSupport {
    /// R1.
    fn id(&self) -> &LanguageId {
        &self.id
    }

    /// R2: a `path_suffixes` entry equal to the file's extension or to its whole file name.
    fn claims(&self, path: &Path) -> bool {
        let _ = path;
        // TODO(reshape-extension-outline): implement
        todo!("ExtensionSupport::claims")
    }

    /// R7: no server.
    fn launch(&self) -> Option<LaunchSpec> {
        // TODO(reshape-extension-outline): implement
        todo!("ExtensionSupport::launch")
    }

    /// No server is started, so there is no narrower root to find: `root` itself.
    fn workspace_root(&self, file: &Path, root: &Path) -> Result<PathBuf, LspError> {
        let _ = (file, root);
        // TODO(reshape-extension-outline): implement
        todo!("ExtensionSupport::workspace_root")
    }

    /// [`ExtensionSupport::outline_backend`] over `root`, whatever `server` is: this tier never
    /// has one.
    fn backend(&self, server: ServerHandle<'_>, root: &Path) -> Box<dyn LanguageBackend> {
        let _ = (server, root);
        // TODO(reshape-extension-outline): implement
        todo!("ExtensionSupport::backend")
    }
}

/// The backend of an extension language with no server: item anchors from the outline, every
/// operation unsupported (R5, R7).
pub struct OutlineBackend {
    #[allow(
        dead_code,
        reason = "TODO(reshape-extension-outline): read by language() once implemented"
    )]
    id: LanguageId,
    #[allow(
        dead_code,
        reason = "TODO(reshape-extension-outline): read by handles_extension once implemented"
    )]
    path_suffixes: Vec<String>,
    #[allow(
        dead_code,
        reason = "TODO(reshape-extension-outline): read by the resolvers once implemented"
    )]
    grammar: Language,
    #[allow(
        dead_code,
        reason = "TODO(reshape-extension-outline): read by the resolvers once implemented"
    )]
    outline: Option<Arc<Query>>,
    #[allow(
        dead_code,
        reason = "TODO(reshape-extension-outline): read by the resolvers once implemented"
    )]
    store: Arc<Mutex<GrammarStore>>,
    /// The tree item anchors' files are relative to, given by
    /// [`LanguageSupport::backend`] — never the process directory.
    #[allow(
        dead_code,
        reason = "TODO(reshape-extension-outline): read by the resolvers once implemented"
    )]
    root: PathBuf,
}

impl LanguageBackend for OutlineBackend {
    fn language(&self) -> &LanguageId {
        // TODO(reshape-extension-outline): implement
        todo!("OutlineBackend::language")
    }

    fn handles_extension(&self, extension: &str) -> bool {
        let _ = extension;
        // TODO(reshape-extension-outline): implement
        todo!("OutlineBackend::handles_extension")
    }

    /// R7: false for every kind, so the registry refuses with `UnsupportedOp { backend: <id> }`.
    fn supports(&self, kind: RefactorKind) -> bool {
        let _ = kind;
        // TODO(reshape-extension-outline): implement
        todo!("OutlineBackend::supports")
    }

    /// Never reached: [`OutlineBackend::supports`] refuses every kind first.
    fn resolve(
        &mut self,
        op: &RefactorOp,
        workspace: &Workspace<'_>,
    ) -> tddy_code_restructuring::Result<Resolution> {
        let _ = (op, workspace);
        // TODO(reshape-extension-outline): implement
        todo!("OutlineBackend::resolve")
    }

    fn item_resolver(&mut self) -> Option<&mut dyn ItemResolver> {
        // TODO(reshape-extension-outline): implement
        todo!("OutlineBackend::item_resolver")
    }

    fn item_locator(&mut self) -> Option<&mut dyn ItemAtResolver> {
        // TODO(reshape-extension-outline): implement
        todo!("OutlineBackend::item_locator")
    }
}

impl ItemResolver for OutlineBackend {
    /// R4/R5: `item` must begin with `file` (the [`ItemResolver::module_of`] below); the segments
    /// after it name exactly one outline entry, else refused as Rust refuses an absent or ambiguous
    /// segment. The fingerprint is [`tddy_code_restructuring::Fingerprint::of`] the entry's whole
    /// lines, as Rust's.
    fn resolve_item(
        &mut self,
        file: &str,
        item: &ItemPath,
    ) -> tddy_code_restructuring::Result<ResolvedItem> {
        let _ = (file, item);
        // TODO(reshape-extension-outline): implement
        todo!("OutlineBackend::resolve_item")
    }

    /// R4: `[<file relative to the root>]` — no module system is known, so the file is the prefix.
    fn module_of(
        &mut self,
        root: &Path,
        file: &str,
    ) -> tddy_code_restructuring::Result<Vec<String>> {
        let _ = (root, file);
        // TODO(reshape-extension-outline): implement
        todo!("OutlineBackend::module_of")
    }
}

impl ItemAtResolver for OutlineBackend {
    /// The innermost entry enclosing `range`, its path `<file>::<path…>::<name>`.
    fn item_enclosing(
        &mut self,
        file: &str,
        range: Range,
    ) -> tddy_code_restructuring::Result<(ItemPath, ResolvedItem)> {
        let _ = (file, range);
        // TODO(reshape-extension-outline): implement
        todo!("OutlineBackend::item_enclosing")
    }
}
