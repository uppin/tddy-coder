//! One language an installed extension declares: `installed/<id>/languages/<x>/config.toml` and
//! the `outline.scm` beside it (PRD F3 a — the only query this tier reads).

use std::path::{Path, PathBuf};

use serde::Deserialize;
use tddy_lsp::LanguageId;

use crate::error::ExtensionError;
use crate::manifest::ExtensionManifest;

/// A language an installed extension declares, as its `languages/<x>/` directory describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionLanguage {
    /// The id of the extension that declares it, for every report about it (R8).
    pub extension: String,
    /// The installed extension's root, `installed/<id>/` — where `grammars/<grammar>.wasm` is.
    pub extension_dir: PathBuf,
    /// `config.toml`'s `name`, e.g. `"TOML"`.
    pub name: String,
    /// `config.toml`'s `grammar`: the grammar loaded from `grammars/<grammar>.wasm`.
    pub grammar: String,
    /// `config.toml`'s `path_suffixes`: a file extension (`toml`) or a whole file name
    /// (`Cargo.lock`) — the only claims the language makes (R2).
    pub path_suffixes: Vec<String>,
    /// The text of `outline.scm`, or `None` when the language ships none (then it has no entries).
    pub outline_query: Option<String>,
}

/// The fields of a language's `config.toml` this tier reads; the rest (brackets, comments, …) are
/// accepted and ignored, as Zed accepts them.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LanguageConfig {
    pub name: String,
    pub grammar: String,
    #[serde(default)]
    pub path_suffixes: Vec<String>,
}

impl ExtensionLanguage {
    /// Every language the extension installed at `extension_dir` declares: the manifest's
    /// `languages` directories, or every directory under `languages/` when it lists none, each
    /// read from its `config.toml` and `outline.scm`, sorted by name.
    ///
    /// A language directory without a `config.toml` is
    /// [`ExtensionError::LanguageConfigMissing`]; an unparsable one is
    /// [`ExtensionError::ManifestUnparsable`] naming that file.
    pub fn load_all(
        extension_dir: &Path,
        manifest: &ExtensionManifest,
    ) -> Result<Vec<ExtensionLanguage>, ExtensionError> {
        let mut languages = Vec::new();
        for relative in language_dirs(extension_dir, manifest)? {
            languages.push(read_language(extension_dir, &relative, &manifest.id)?);
        }
        languages.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(languages)
    }

    /// The support id (R1): the language name lower-cased, each space a `-` — `"Elixir"` →
    /// `elixir`, `"Demo TOML"` → `demo-toml`.
    pub fn support_id(&self) -> LanguageId {
        LanguageId::new(&self.name.to_lowercase().replace(' ', "-"))
    }

    /// `<extension_dir>/grammars/<grammar>.wasm`.
    pub fn grammar_wasm(&self) -> PathBuf {
        self.extension_dir
            .join("grammars")
            .join(format!("{}.wasm", self.grammar))
    }
}

/// The language directories, relative to the extension root: the manifest's `languages`, else every
/// directory under `languages/` (none when there is no such directory).
fn language_dirs(
    extension_dir: &Path,
    manifest: &ExtensionManifest,
) -> Result<Vec<PathBuf>, ExtensionError> {
    if !manifest.languages.is_empty() {
        return Ok(manifest.languages.clone());
    }
    let languages = extension_dir.join("languages");
    let entries = match std::fs::read_dir(&languages) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(ExtensionError::io(&languages, error)),
    };
    let mut dirs = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| ExtensionError::io(&languages, error))?;
        if entry.path().is_dir() {
            dirs.push(Path::new("languages").join(entry.file_name()));
        }
    }
    dirs.sort();
    Ok(dirs)
}

/// The language the directory `relative` (to `extension_dir`) holds: its `config.toml` and, if it
/// has one, its `outline.scm`.
fn read_language(
    extension_dir: &Path,
    relative: &Path,
    extension: &str,
) -> Result<ExtensionLanguage, ExtensionError> {
    let config_file = relative.join("config.toml");
    let config_path = extension_dir.join(&config_file);
    let text = match std::fs::read_to_string(&config_path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(ExtensionError::LanguageConfigMissing {
                extension: extension.to_string(),
                file: config_file,
            })
        }
        Err(error) => return Err(ExtensionError::io(&config_path, error)),
    };
    let config: LanguageConfig =
        toml::from_str(&text).map_err(|error| ExtensionError::ManifestUnparsable {
            extension: extension.to_string(),
            file: config_file,
            message: error.to_string(),
        })?;

    let outline_path = extension_dir.join(relative).join("outline.scm");
    let outline_query = match std::fs::read_to_string(&outline_path) {
        Ok(query) => Some(query),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(ExtensionError::io(&outline_path, error)),
    };

    Ok(ExtensionLanguage {
        extension: extension.to_string(),
        extension_dir: extension_dir.to_path_buf(),
        name: config.name,
        grammar: config.grammar,
        path_suffixes: config.path_suffixes,
        outline_query,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn a_language_named(name: &str) -> ExtensionLanguage {
        ExtensionLanguage {
            extension: "an-extension".into(),
            extension_dir: PathBuf::from("/extensions/installed/an-extension"),
            name: name.into(),
            grammar: "a-grammar".into(),
            path_suffixes: vec![],
            outline_query: None,
        }
    }

    fn a_manifest(text: &str) -> ExtensionManifest {
        ExtensionManifest::parse(text).expect("the manifest parses")
    }

    /// An extension directory holding `files` (path relative to it, contents).
    fn an_extension_dir_holding(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::TempDir::new().expect("temp extension dir");
        for (path, text) in files {
            let path = dir.path().join(path);
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("parent dirs");
            std::fs::write(path, text).expect("write a fixture file");
        }
        dir
    }

    const MANIFEST: &str = "id = \"an-extension\"\nname = \"An\"\nversion = \"1.0.0\"\n";

    #[test]
    fn the_support_id_is_the_name_lower_cased_with_dashes_for_spaces() {
        // When the R1 id is taken of single- and multi-word names
        let ids = [
            a_language_named("Elixir").support_id(),
            a_language_named("Demo TOML").support_id(),
            a_language_named("Git Commit Message").support_id(),
        ];

        // Then each is the name lower-cased, each space a dash
        assert_eq!(
            ids.map(|id| id.to_string()),
            ["elixir", "demo-toml", "git-commit-message"].map(String::from)
        );
    }

    #[test]
    fn languages_are_read_from_every_directory_under_languages_when_the_manifest_lists_none() {
        // Given an extension with two language directories, one with an outline
        let dir = an_extension_dir_holding(&[
            (
                "languages/beta/config.toml",
                "name = \"Beta\"\ngrammar = \"beta\"\npath_suffixes = [\"b\"]\nline_comments = [\"# \"]\n",
            ),
            ("languages/beta/outline.scm", "(item) @item"),
            (
                "languages/alpha/config.toml",
                "name = \"Alpha\"\ngrammar = \"alpha\"\npath_suffixes = [\"a\", \"Alphafile\"]\n",
            ),
        ]);

        // When its languages are loaded
        let languages =
            ExtensionLanguage::load_all(dir.path(), &a_manifest(MANIFEST)).expect("they load");

        // Then both are read, sorted by name, each with its suffixes and its outline if it has one
        assert_eq!(
            languages
                .iter()
                .map(|language| (
                    language.name.as_str(),
                    language.grammar.as_str(),
                    language.path_suffixes.clone(),
                    language.outline_query.clone(),
                ))
                .collect::<Vec<_>>(),
            vec![
                (
                    "Alpha",
                    "alpha",
                    vec!["a".to_string(), "Alphafile".to_string()],
                    None
                ),
                (
                    "Beta",
                    "beta",
                    vec!["b".to_string()],
                    Some("(item) @item".to_string())
                ),
            ]
        );
    }

    #[test]
    fn a_manifest_that_lists_its_languages_is_read_only_there() {
        // Given a manifest naming `languages/kept`, and a second directory it does not name
        let dir = an_extension_dir_holding(&[
            (
                "languages/kept/config.toml",
                "name = \"Kept\"\ngrammar = \"kept\"\npath_suffixes = [\"k\"]\n",
            ),
            (
                "languages/unlisted/config.toml",
                "name = \"Unlisted\"\ngrammar = \"unlisted\"\n",
            ),
        ]);
        let manifest = a_manifest(&format!("{MANIFEST}languages = [\"languages/kept\"]\n"));

        // When its languages are loaded
        let languages = ExtensionLanguage::load_all(dir.path(), &manifest).expect("they load");

        // Then only the listed one is
        assert_eq!(
            languages
                .iter()
                .map(|language| language.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Kept"]
        );
    }

    #[test]
    fn a_language_directory_without_a_config_is_refused_naming_it() {
        // Given a language directory holding only an outline
        let dir = an_extension_dir_holding(&[("languages/bare/outline.scm", "(item) @item")]);

        // When its languages are loaded
        let refused = ExtensionLanguage::load_all(dir.path(), &a_manifest(MANIFEST));

        // Then the missing config is named, relative to the extension root
        assert_eq!(
            refused,
            Err(ExtensionError::LanguageConfigMissing {
                extension: "an-extension".into(),
                file: PathBuf::from("languages/bare/config.toml"),
            })
        );
    }
}
