//! `extension.toml`, the fields of Zed's `ExtensionManifest` (`crates/extension/src/extension_manifest.rs`)
//! that tddy reads. Unknown fields (themes, capabilities, snippets, …) are accepted and ignored, as
//! Zed itself does, so any published extension parses.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::ExtensionError;

/// The schema versions tddy installs (R3).
pub const SUPPORTED_SCHEMA_VERSIONS: std::ops::RangeInclusive<i32> = 0..=1;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ExtensionManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    /// Absent in the oldest extensions, which Zed reads as 0.
    #[serde(default)]
    pub schema_version: i32,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub lib: LibEntry,
    /// Language directories, relative to the extension root. Usually empty: Zed then scans
    /// `languages/`.
    #[serde(default)]
    pub languages: Vec<PathBuf>,
    #[serde(default)]
    pub grammars: BTreeMap<String, GrammarEntry>,
    #[serde(default)]
    pub language_servers: BTreeMap<String, LanguageServerEntry>,
}

/// `[lib]`: present when the extension ships code (`extension.wasm`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct LibEntry {
    #[serde(default)]
    pub kind: Option<LibKind>,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum LibKind {
    Rust,
}

/// `[grammars.<name>]`: where Zed builds the grammar from. tddy builds nothing — it requires
/// `grammars/<name>.wasm` in the archive — but keeps the provenance.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct GrammarEntry {
    pub repository: String,
    #[serde(alias = "commit")]
    pub rev: String,
    #[serde(default)]
    pub path: Option<String>,
}

/// `[language_servers.<id>]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct LanguageServerEntry {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default)]
    pub language_ids: BTreeMap<String, String>,
    /// Languages (from `languages`) the server does not start for unless a setting names it — Zed's
    /// `opt_in_languages`.
    #[serde(default)]
    pub opt_in_languages: BTreeSet<String>,
    /// The code-action kinds the server is asked for — Zed's `code_action_kinds`.
    #[serde(default)]
    pub code_action_kinds: Option<Vec<String>>,
}

impl ExtensionManifest {
    /// Parses the text of an `extension.toml`.
    pub fn parse(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    /// Refuses a `schema_version` outside [`SUPPORTED_SCHEMA_VERSIONS`], naming this extension and
    /// `file` (relative to the extension root).
    pub fn check_schema_version(&self, file: &Path) -> Result<(), ExtensionError> {
        let _ = file;
        // TODO(reshape-extension-install): implement
        todo!("ExtensionManifest::check_schema_version")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    const A_CODE_EXTENSION: &str = r#"
        id = "demo-lang"
        name = "Demo Language"
        version = "0.3.1"
        schema_version = 1
        authors = ["A <a@example.invalid>"]
        description = "Demo"
        repository = "https://example.invalid/demo-lang"

        [lib]
        kind = "Rust"
        version = "0.6.0"

        [grammars.demo]
        repository = "https://example.invalid/tree-sitter-demo"
        commit = "abc123"

        [language_servers.demo-ls]
        name = "Demo LS"
        languages = ["Demo"]

        [themes]
        ignored = "an unknown table is accepted, as Zed accepts it"
    "#;

    #[test]
    fn manifest_parses_zeds_fields() {
        // When
        let manifest = ExtensionManifest::parse(A_CODE_EXTENSION).expect("parses");

        // Then
        assert_eq!(
            manifest,
            ExtensionManifest {
                id: "demo-lang".into(),
                name: "Demo Language".into(),
                version: "0.3.1".into(),
                schema_version: 1,
                description: Some("Demo".into()),
                repository: Some("https://example.invalid/demo-lang".into()),
                authors: vec!["A <a@example.invalid>".into()],
                lib: LibEntry {
                    kind: Some(LibKind::Rust),
                    version: Some("0.6.0".into()),
                },
                languages: vec![],
                grammars: BTreeMap::from([(
                    "demo".into(),
                    GrammarEntry {
                        repository: "https://example.invalid/tree-sitter-demo".into(),
                        rev: "abc123".into(),
                        path: None,
                    },
                )]),
                language_servers: BTreeMap::from([(
                    "demo-ls".into(),
                    LanguageServerEntry {
                        name: Some("Demo LS".into()),
                        languages: vec!["Demo".into()],
                        ..LanguageServerEntry::default()
                    },
                )]),
            }
        );
    }

    #[test]
    fn a_language_server_entry_reads_its_opt_in_languages_ids_and_action_kinds() {
        // Given a server serving two languages, one only when a setting names it
        let manifest = ExtensionManifest::parse(
            r#"
                id = "demo-lang"
                name = "Demo"
                version = "0.1.0"
                schema_version = 1

                [language_servers.demo-ls]
                languages = ["Demo", "Demo Script"]
                opt_in_languages = ["Demo Script"]
                language_ids = { "Demo Script" = "demo-script" }
                code_action_kinds = ["refactor.extract"]
            "#,
        )
        .expect("parses");

        // Then the entry carries all three, as Zed's manifest does
        assert_eq!(
            manifest.language_servers["demo-ls"],
            LanguageServerEntry {
                languages: vec!["Demo".into(), "Demo Script".into()],
                language_ids: BTreeMap::from([("Demo Script".into(), "demo-script".into())]),
                opt_in_languages: BTreeSet::from(["Demo Script".into()]),
                code_action_kinds: Some(vec!["refactor.extract".into()]),
                ..LanguageServerEntry::default()
            }
        );
    }

    #[test]
    fn schema_version_outside_the_range_is_refused() {
        // Given
        let manifest = ExtensionManifest::parse(
            r#"
                id = "future-lang"
                name = "Future"
                version = "1.0.0"
                schema_version = 2
            "#,
        )
        .expect("parses");

        // When
        let refusal = manifest.check_schema_version(Path::new("extension.toml"));

        // Then
        assert_eq!(
            refusal,
            Err(ExtensionError::SchemaVersionOutOfRange {
                extension: "future-lang".into(),
                file: PathBuf::from("extension.toml"),
                schema_version: 2,
            })
        );
    }
}
