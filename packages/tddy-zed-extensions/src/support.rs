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

use tddy_code_restructuring::backends::ServerBackend;
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
use crate::server::ServerChoice;

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
        let manifest = read_installed_manifest(&extension_dir)?;
        for language in ExtensionLanguage::load_all(&extension_dir, &manifest)? {
            languages.push((manifest.clone(), language));
        }
    }

    // No language, no engine: a run with no extension installed never starts wasmtime.
    if languages.is_empty() {
        return Ok(InstalledLanguages {
            supports: Vec::new(),
            failures: Vec::new(),
        });
    }
    let languages = crate::server::served(dir, languages)?;
    let store = GrammarStore::new().map_err(|error| ExtensionError::GrammarEngineUnavailable {
        message: error.message,
    })?;
    Ok(supports_of(languages, Arc::new(Mutex::new(store))))
}

/// The `extension.toml` of the extension installed at `extension_dir`, named in every refusal by
/// its directory — the id it was installed under.
fn read_installed_manifest(extension_dir: &Path) -> Result<ExtensionManifest, ExtensionError> {
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
    ExtensionManifest::parse(&text).map_err(|error| ExtensionError::ManifestUnparsable {
        extension: id,
        file: PathBuf::from("extension.toml"),
        message: error.to_string(),
    })
}

/// A support per language whose grammar loads into `store` and whose `outline.scm` compiles, each
/// served as its [`ServerChoice`] says, and one report per failure (R8).
///
/// A grammar that does not load fails every language on it, and is reported once however many
/// share it. A language's own failure — an `outline.scm` that does not compile — fails that
/// language alone: a sibling on the same grammar still loads.
fn supports_of(
    languages: Vec<crate::server::Served>,
    store: Arc<Mutex<GrammarStore>>,
) -> InstalledLanguages {
    let mut supports = Vec::new();
    let mut failures: Vec<GrammarLoadError> = Vec::new();
    let mut failed_grammars: BTreeSet<(String, String)> = BTreeSet::new();
    for (language, server, kinds) in languages {
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
        let support = match loaded {
            Ok(_) => ExtensionSupport::new(language, Arc::clone(&store))
                .map(|support| support.with_server(server).with_code_action_kinds(kinds)),
            Err(failure) => {
                failed_grammars.insert(grammar);
                Err(failure)
            }
        };
        match support {
            Ok(support) => supports.push(support),
            // The same failure from a sibling (a grammar no parser can use) is reported once.
            Err(failure) if !failures.contains(&failure) => failures.push(failure),
            Err(_) => {}
        }
    }
    InstalledLanguages { supports, failures }
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
///
/// Cloned cheaply — the grammar, the compiled outline and the store are shared — so a front end can
/// register one in the run's [`SupportRegistry`] and keep another for its [`crate::ExtensionVerify`].
#[derive(Clone)]
pub struct ExtensionSupport {
    id: LanguageId,
    language: ExtensionLanguage,
    grammar: Language,
    /// `outline.scm` compiled against `grammar`; `None` when the language ships none.
    outline: Option<Arc<Query>>,
    /// The process's one store, shared by every extension language.
    store: Arc<Mutex<GrammarStore>>,
    /// What serves the language (`#reshape` extension-language-server, R1–R3):
    /// [`ServerChoice::Undeclared`] until [`ExtensionSupport::with_server`] says otherwise.
    server: ServerChoice,
    /// The kinds `apply_code_action` asks the language's server for; empty asks for every kind.
    code_action_kinds: Vec<String>,
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
            server: ServerChoice::Undeclared,
            code_action_kinds: Vec::new(),
        })
    }

    /// This support, whose server is asked for code actions of `kinds` only (`context.only`) —
    /// the manifest's `code_action_kinds` for the server; none asks for every kind.
    pub fn with_code_action_kinds(self, kinds: Vec<String>) -> Self {
        Self {
            code_action_kinds: kinds,
            ..self
        }
    }

    /// This support, served as `server` says ([`load_installed`] gives every support its
    /// [`ServerChoice::for_language`]).
    pub fn with_server(self, server: ServerChoice) -> Self {
        Self { server, ..self }
    }

    /// What serves this support's language.
    pub fn server(&self) -> &ServerChoice {
        &self.server
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

    /// The configured server's launch ([`ServerChoice::launch`]); `None` for a language with no
    /// server, or one whose server is unconfigured or refused.
    fn launch(&self) -> Option<LaunchSpec> {
        self.server.launch().cloned()
    }

    /// No server is started, so there is no narrower root to find: `root` itself.
    fn workspace_root(&self, file: &Path, root: &Path) -> Result<PathBuf, LspError> {
        let _ = file;
        Ok(root.to_path_buf())
    }

    /// [`ExtensionSupport::outline_backend`] over `root`, composed with a [`ServerBackend`] when the
    /// language has a server: over the live one handed in, or — for an unconfigured or refused one —
    /// without a command, so a server operation is refused in its words before anything starts. A
    /// configured server with none live (the static tier) answers item anchors alone.
    fn backend(&self, server: ServerHandle<'_>, root: &Path) -> Box<dyn LanguageBackend> {
        let outline = Box::new(self.outline_backend(root));
        match (&self.server, self.server.refusal(), server.server()) {
            (_, Some(reason), _) => Box::new(ServerBackend::without_command(outline, reason)),
            (ServerChoice::Configured { launch, .. }, None, Some(live)) => {
                let language_id = match launch.language_id.as_str() {
                    "" => self.id.as_str(),
                    declared => declared,
                };
                Box::new(
                    ServerBackend::over(outline, live, language_id)
                        .with_code_action_kinds(self.code_action_kinds.clone()),
                )
            }
            _ => outline,
        }
    }
}

/// The parse a verify reads (`#reshape` extension-code-actions, R5): owned by that node, beside
/// the support whose private grammar and store it parses with.
impl ExtensionSupport {
    /// `text` parsed with this language's grammar, through the process's one store; `None` only
    /// when tree-sitter gives up on the text altogether.
    pub fn parse(&self, text: &str) -> Option<tree_sitter::Tree> {
        self.store
            .lock()
            .expect("the grammar store is not poisoned")
            .parse(&self.grammar, text)
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
        // TODO: a whole-file-name suffix (`Cargo.lock`) is claimed by `ExtensionSupport::claims`
        // but the registry routes by extension alone, so only the extension form reaches this
        // backend — docs/dev/todo/2026-10-22-extension-whole-file-name-suffix-is-not-routed.md
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

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    /// `installed/two-tomls/` with two languages on the vendored TOML grammar: `Good TOML`, whose
    /// `outline.scm` compiles, and `Bad TOML`, whose `outline.scm` names a node TOML has not.
    fn an_extension_whose_two_languages_share_a_grammar() -> (tempfile::TempDir, ExtensionsDir) {
        let data = tempfile::TempDir::new().expect("temp data dir");
        let dir = ExtensionsDir::under_data_dir(data.path());
        let root = dir.extension("two-tomls");
        std::fs::create_dir_all(root.join("grammars")).expect("grammars dir");
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/grammars/tree-sitter-toml/toml.wasm"),
            root.join("grammars/toml.wasm"),
        )
        .expect("copy the vendored grammar");
        std::fs::write(
            root.join("extension.toml"),
            "id = \"two-tomls\"\nname = \"Two TOMLs\"\nversion = \"0.1.0\"\nschema_version = 1\n",
        )
        .expect("write the manifest");
        for (language, query) in [
            ("good-toml", "(pair (_) @name \"=\") @item"),
            ("bad-toml", "(no_such_node) @item"),
        ] {
            let language_dir = root.join("languages").join(language);
            std::fs::create_dir_all(&language_dir).expect("language dir");
            let name = if language == "good-toml" {
                "Good TOML"
            } else {
                "Bad TOML"
            };
            std::fs::write(
                language_dir.join("config.toml"),
                format!("name = \"{name}\"\ngrammar = \"toml\"\npath_suffixes = [\"toml\"]\n"),
            )
            .expect("write the language config");
            std::fs::write(language_dir.join("outline.scm"), query).expect("write the query");
        }
        (data, dir)
    }

    #[test]
    fn an_outline_that_does_not_compile_fails_its_language_and_spares_a_sibling_on_its_grammar() {
        // Given an extension whose two languages share a grammar, one with a broken outline.scm
        let (_data, dir) = an_extension_whose_two_languages_share_a_grammar();

        // When its languages are loaded
        let languages = load_installed(&dir).expect("one broken outline fails no run");

        // Then the broken one is reported, and the other still loads on the same grammar
        let loaded: Vec<String> = languages
            .supports
            .iter()
            .map(|support| support.id().to_string())
            .collect();
        let reported: Vec<(String, String, bool)> = languages
            .failures
            .iter()
            .map(|failure| {
                (
                    failure.extension.clone(),
                    failure.grammar.clone(),
                    failure
                        .reason
                        .starts_with("its outline.scm does not compile"),
                )
            })
            .collect();
        assert_eq!(
            (loaded, reported),
            (
                vec!["good-toml".to_string()],
                vec![("two-tomls".to_string(), "toml".to_string(), true)]
            )
        );
    }
}
