//! An installed extension's languages as language supports (K11's interface): each claims its
//! `path_suffixes`, runs no server, and answers item anchors from its outline.
//!
//! Item anchors for these languages go through the same item-anchor traits Rust's do —
//! [`ItemResolver`] and [`ItemAtResolver`], routed by the [`BackendRegistry`] — so
//! `restructure anchors --items` / `--at`, an `items` / `item` anchor in a plan, and its
//! fingerprint check need nothing language-specific above this module.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tddy_code_restructuring::item_anchor::{ItemAtResolver, ItemResolver, ResolvedItem};
use tddy_code_restructuring::registry::Workspace;
use tddy_code_restructuring::{
    Fingerprint, ItemPath, ItemSegment, LanguageBackend, LanguageSupport, Range, RefactorKind,
    RefactorOp, Resolution, RestructureError, ServerHandle, SupportRegistry,
};
use tddy_lsp::{LanguageId, LaunchSpec, LspError};
use tree_sitter::{Language, Query};

use crate::dir::ExtensionsDir;
use crate::error::ExtensionError;
use crate::grammar::{GrammarLoadError, GrammarStore};
use crate::language::ExtensionLanguage;
use crate::manifest::ExtensionManifest;
use crate::outline::{Outline, OutlineEntry};

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
    let mut languages = Vec::new();
    for extension_dir in installed_extension_dirs(dir)? {
        let manifest_file = extension_dir.join("extension.toml");
        let id = extension_dir
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        let text = std::fs::read_to_string(&manifest_file).map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => ExtensionError::ManifestMissing {
                extension: id.clone(),
                file: PathBuf::from("extension.toml"),
            },
            _ => ExtensionError::io(&manifest_file, error),
        })?;
        let manifest = ExtensionManifest::parse(&text).map_err(|error| {
            ExtensionError::ManifestUnparsable {
                extension: id.clone(),
                file: PathBuf::from("extension.toml"),
                message: error.to_string(),
            }
        })?;
        languages.extend(ExtensionLanguage::load_all(&extension_dir, &manifest)?);
    }

    // No language, no engine: a run with no extension installed never starts wasmtime.
    if languages.is_empty() {
        return Ok(InstalledLanguages {
            supports: Vec::new(),
            failures: Vec::new(),
        });
    }
    let store = GrammarStore::new().map_err(|error| ExtensionError::Io {
        path: dir.installed(),
        message: format!("the WASM engine did not start: {}", error.message),
    })?;
    let store = Arc::new(Mutex::new(store));

    let mut supports = Vec::new();
    let mut failures: Vec<GrammarLoadError> = Vec::new();
    let mut failed_grammars: BTreeSet<(String, String)> = BTreeSet::new();
    for language in languages {
        let grammar = (language.extension.clone(), language.grammar.clone());
        if failed_grammars.contains(&grammar) {
            continue;
        }
        let loaded = store
            .lock()
            .expect("the grammar store is not poisoned")
            .load(
                &language.extension,
                &language.extension_dir,
                &language.grammar,
            );
        let support = loaded.and_then(|_| ExtensionSupport::new(language, Arc::clone(&store)));
        match support {
            Ok(support) => supports.push(support),
            Err(failure) => {
                // However many languages share a grammar, it is reported once.
                failed_grammars.insert(grammar);
                if !failures.contains(&failure) {
                    failures.push(failure);
                }
            }
        }
    }
    Ok(InstalledLanguages { supports, failures })
}

/// `installed/<id>/` for every extension, sorted by id, without the dot-directories a failed
/// install leaves. No `installed/` is no extensions.
fn installed_extension_dirs(dir: &ExtensionsDir) -> Result<Vec<PathBuf>, ExtensionError> {
    let installed = dir.installed();
    let entries = match std::fs::read_dir(&installed) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(ExtensionError::io(&installed, error)),
    };
    let mut dirs = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| ExtensionError::io(&installed, error))?;
        let hidden = entry.file_name().to_string_lossy().starts_with('.');
        if !hidden && entry.path().is_dir() {
            dirs.push(entry.path());
        }
    }
    dirs.sort();
    Ok(dirs)
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
        let installed = load_installed(dir)?;
        for support in installed.supports {
            self.register(Box::new(support));
        }
        Ok(installed.failures)
    }
}

/// One installed extension language as a [`LanguageSupport`].
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
        let failed = |reason: String| GrammarLoadError {
            extension: language.extension.clone(),
            grammar: language.grammar.clone(),
            reason,
        };
        let grammar = store
            .lock()
            .expect("the grammar store is not poisoned")
            .load(
                &language.extension,
                &language.extension_dir,
                &language.grammar,
            )?;
        let outline = language
            .outline_query
            .as_deref()
            .map(|source| Query::new(&grammar, source))
            .transpose()
            .map_err(|error| failed(format!("its outline.scm does not compile: {error}")))?
            .map(Arc::new);
        // A grammar a parser cannot be given (an ABI it does not take) fails here, once, and not
        // as an empty outline on every file.
        if outline_of_text(&store, &grammar, None, "").is_none() {
            return Err(failed("a parser cannot use it".to_string()));
        }
        Ok(Self {
            id: language.support_id(),
            language,
            grammar,
            outline,
            store,
        })
    }

    /// The language this support was built from.
    pub fn language(&self) -> &ExtensionLanguage {
        &self.language
    }

    /// The outline of `text`, a file of this language (R3).
    pub fn outline_of(&self, text: &str) -> Outline {
        outline_of_text(&self.store, &self.grammar, self.outline.as_deref(), text)
            .expect("a grammar that loaded and parsed an empty file parses every file")
    }

    /// The outline backend over this language, reading its files under `root` — the tree item
    /// anchors' files are relative to.
    pub fn outline_backend(&self, root: &Path) -> OutlineBackend {
        OutlineBackend {
            id: self.id.clone(),
            path_suffixes: self.language.path_suffixes.clone(),
            grammar: self.grammar.clone(),
            outline: self.outline.clone(),
            store: Arc::clone(&self.store),
            root: root.to_path_buf(),
        }
    }
}

impl LanguageSupport for ExtensionSupport {
    /// R1.
    fn id(&self) -> &LanguageId {
        &self.id
    }

    /// R2: a `path_suffixes` entry equal to the file's extension or to its whole file name.
    fn claims(&self, path: &Path) -> bool {
        let extension = path.extension().and_then(|extension| extension.to_str());
        let name = path.file_name().and_then(|name| name.to_str());
        self.language
            .path_suffixes
            .iter()
            .any(|suffix| Some(suffix.as_str()) == extension || Some(suffix.as_str()) == name)
    }

    /// R7: no server.
    fn launch(&self) -> Option<LaunchSpec> {
        None
    }

    /// No server is started, so there is no narrower root to find: `root` itself.
    fn workspace_root(&self, file: &Path, root: &Path) -> Result<PathBuf, LspError> {
        let _ = file;
        Ok(root.to_path_buf())
    }

    /// [`ExtensionSupport::outline_backend`] over `root`, whatever `server` is: this tier never
    /// has one.
    fn backend(&self, server: ServerHandle<'_>, root: &Path) -> Box<dyn LanguageBackend> {
        let _ = server;
        Box::new(self.outline_backend(root))
    }
}

/// The backend of an extension language with no server: item anchors from the outline, every
/// operation unsupported (R5, R7).
pub struct OutlineBackend {
    id: LanguageId,
    path_suffixes: Vec<String>,
    grammar: Language,
    outline: Option<Arc<Query>>,
    store: Arc<Mutex<GrammarStore>>,
    /// The tree item anchors' files are relative to, given by
    /// [`LanguageSupport::backend`] — never the process directory.
    root: PathBuf,
}

impl LanguageBackend for OutlineBackend {
    fn language(&self) -> &LanguageId {
        &self.id
    }

    fn handles_extension(&self, extension: &str) -> bool {
        // TODO(reshape-extension-outline): a whole-file-name suffix (`Cargo.lock`) is claimed by
        // `ExtensionSupport::claims` but the registry routes by extension alone, so only the
        // extension form reaches this backend.
        self.path_suffixes.iter().any(|suffix| suffix == extension)
    }

    /// R7: false for every kind, so the registry refuses with `UnsupportedOp { backend: <id> }`.
    fn supports(&self, kind: RefactorKind) -> bool {
        let _ = kind;
        false
    }

    /// Never reached: [`OutlineBackend::supports`] refuses every kind first.
    fn resolve(
        &mut self,
        op: &RefactorOp,
        workspace: &Workspace<'_>,
    ) -> tddy_code_restructuring::Result<Resolution> {
        let _ = workspace;
        Err(RestructureError::UnsupportedOp {
            backend: self.id.to_string(),
            op: format!("{:?}", op.op),
        })
    }

    fn item_resolver(&mut self) -> Option<&mut dyn ItemResolver> {
        Some(self)
    }

    fn item_locator(&mut self) -> Option<&mut dyn ItemAtResolver> {
        Some(self)
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
        let (text, outline) = self.read_outline(file)?;
        let segments = segments_below(file, item)?;
        match outline.entries_at(&segments).as_slice() {
            [entry] => Ok(resolved_from(&text, entry)),
            [] => {
                // The first segment nothing answers to, as Rust's refusal names it.
                let absent = (1..=segments.len())
                    .find(|count| outline.entries_at(&segments[..*count]).is_empty())
                    .unwrap_or(segments.len());
                Err(RestructureError::MalformedPlan(format!(
                    "`{item}` is not declared in {file}: nothing there is named `{}`",
                    segments[absent - 1]
                )))
            }
            many => Err(RestructureError::MalformedPlan(format!(
                "`{item}` names {} items in {file}",
                many.len()
            ))),
        }
    }

    /// R4: `[<file relative to the root>]` — no module system is known, so the file is the prefix.
    fn module_of(
        &mut self,
        root: &Path,
        file: &str,
    ) -> tddy_code_restructuring::Result<Vec<String>> {
        let _ = root;
        Ok(vec![file.to_string()])
    }
}

impl ItemAtResolver for OutlineBackend {
    /// The innermost entry enclosing `range`, its path `<file>::<path…>::<name>`.
    fn item_enclosing(
        &mut self,
        file: &str,
        range: Range,
    ) -> tddy_code_restructuring::Result<(ItemPath, ResolvedItem)> {
        let (text, outline) = self.read_outline(file)?;
        let entry = outline.innermost_enclosing(range).ok_or_else(|| {
            RestructureError::MalformedPlan(format!(
                "no item of {file} encloses {}:{}",
                range.start.line, range.start.col
            ))
        })?;
        let item = ItemPath::parse(&format!("{file}::{}", entry.segments().join("::")))?;
        Ok((item, resolved_from(&text, entry)))
    }
}

impl OutlineBackend {
    /// The text of `file` under the root this backend was built over, and its outline.
    fn read_outline(&self, file: &str) -> tddy_code_restructuring::Result<(String, Outline)> {
        let text = std::fs::read_to_string(self.root.join(file))?;
        let outline = outline_of_text(&self.store, &self.grammar, self.outline.as_deref(), &text)
            .ok_or_else(|| {
            RestructureError::ServerDefect(format!(
                "the grammar of {} produced no parse of {file}",
                self.id
            ))
        })?;
        Ok((text, outline))
    }
}

/// `text` parsed and outlined; an empty outline when the language ships no `outline.scm`. `None`
/// only when the grammar cannot be given to a parser.
fn outline_of_text(
    store: &Mutex<GrammarStore>,
    grammar: &Language,
    query: Option<&Query>,
    text: &str,
) -> Option<Outline> {
    let tree = store
        .lock()
        .expect("the grammar store is not poisoned")
        .parse(grammar, text)?;
    Some(query.map_or_else(Outline::default, |query| Outline::of(&tree, text, query)))
}

/// The segments of `item` below `file` (R4): `item` must begin with the file itself.
fn segments_below(file: &str, item: &ItemPath) -> tddy_code_restructuring::Result<Vec<String>> {
    if item.crate_name() != file {
        return Err(RestructureError::MalformedPlan(format!(
            "`{item}` does not begin with `{file}`: an item of an extension language is written \
             `{file}::<item>`"
        )));
    }
    Ok(item.segments().iter().map(ItemSegment::spelled).collect())
}

/// The entry's extent and name, with the fingerprint of its whole lines as Rust's items have.
fn resolved_from(text: &str, entry: &OutlineEntry) -> ResolvedItem {
    let lines: Vec<&str> = text
        .split('\n')
        .skip(entry.span.start.line as usize - 1)
        .take((entry.span.end.line - entry.span.start.line) as usize + 1)
        .collect();
    ResolvedItem {
        range: entry.span,
        name: entry.name_at,
        fingerprint: Fingerprint::of(&lines.join("\n")),
    }
}
