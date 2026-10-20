//! `<extensions dir>/settings.json`: what tddy starts an extension language's server with, in the
//! shape of Zed's own `lsp` setting (PRD F3 a), so a block copied from Zed's `settings.json` works
//! here unchanged:
//!
//! ```json
//! { "lsp": { "elixir-ls": { "binary": { "path": "/opt/elixir-ls/language_server.sh",
//!                                        "arguments": [], "env": {} },
//!                            "initialization_options": {}, "settings": {} } } }
//! ```
//!
//! Zed gets a server's command from the extension's WASM unless the user set
//! `lsp.<id>.binary.path`; tddy runs no WASM here, so that setting is the only command there is
//! (R2). Every key this tier does not read is accepted and ignored, as Zed accepts it.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Deserialize;
use serde_json::Value;

use crate::dir::ExtensionsDir;
use crate::error::ExtensionError;

/// The settings file's name inside the extensions directory.
pub const SETTINGS_FILE: &str = "settings.json";

/// The whole settings file. Only `lsp` is read.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct ExtensionSettings {
    /// One entry per language server id — the id of a manifest's `[language_servers.<id>]`.
    #[serde(default)]
    pub lsp: BTreeMap<String, ServerSettings>,
}

/// `lsp.<id>`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct ServerSettings {
    /// How the server starts; without a `path` it has no command (R3).
    #[serde(default)]
    pub binary: Option<BinarySettings>,
    /// Sent as `initializationOptions` at `initialize` (R4); null when absent.
    #[serde(default)]
    pub initialization_options: Value,
    /// What the server's `workspace/configuration` requests are answered from (R4); null when
    /// absent.
    #[serde(default)]
    pub settings: Value,
}

/// `lsp.<id>.binary`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct BinarySettings {
    /// The program, run as written: never looked up on `PATH` by the server's id (PRD F1 a).
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub arguments: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

impl ExtensionSettings {
    /// `<dir>/settings.json`.
    pub fn file(dir: &ExtensionsDir) -> PathBuf {
        dir.root().join(SETTINGS_FILE)
    }

    /// Parses the text of a settings file; keys other than `lsp` and its documented fields are
    /// ignored.
    pub fn parse(text: &str) -> Result<Self, serde_json::Error> {
        let _ = text;
        // TODO(reshape-extension-language-server): implement
        todo!("ExtensionSettings::parse")
    }

    /// The settings at [`ExtensionSettings::file`]. No file is no settings — every server
    /// unconfigured; a file that cannot be read or does not parse is [`ExtensionError::Io`] naming
    /// it, never silently empty.
    pub fn load(dir: &ExtensionsDir) -> Result<Self, ExtensionError> {
        let _ = dir;
        // TODO(reshape-extension-language-server): implement
        todo!("ExtensionSettings::load")
    }

    /// `lsp.<server>`, if the file has one.
    pub fn server(&self, server: &str) -> Option<&ServerSettings> {
        let _ = server;
        // TODO(reshape-extension-language-server): implement
        todo!("ExtensionSettings::server")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    /// An extensions dir under a temp data dir; dropping the dir deletes it.
    fn an_extensions_dir() -> (tempfile::TempDir, ExtensionsDir) {
        let data = tempfile::TempDir::new().expect("temp data dir");
        let dir = ExtensionsDir::under_data_dir(data.path());
        std::fs::create_dir_all(dir.root()).expect("extensions dir");
        (data, dir)
    }

    #[test]
    fn a_zed_lsp_block_parses_unchanged() {
        // Given a block as Zed's own settings.json spells it
        let text = json!({
            "lsp": {
                "elixir-ls": {
                    "binary": {
                        "path": "/opt/elixir-ls/language_server.sh",
                        "arguments": ["--stdio"],
                        "env": { "MIX_ENV": "dev" }
                    },
                    "initialization_options": { "dialyzerEnabled": false },
                    "settings": { "elixirLS": { "fetchDeps": false } }
                }
            }
        })
        .to_string();

        // When it is parsed
        let settings = ExtensionSettings::parse(&text).expect("the block parses");

        // Then every field the server is started with is read
        assert_eq!(
            settings,
            ExtensionSettings {
                lsp: BTreeMap::from([(
                    "elixir-ls".to_string(),
                    ServerSettings {
                        binary: Some(BinarySettings {
                            path: Some("/opt/elixir-ls/language_server.sh".to_string()),
                            arguments: vec!["--stdio".to_string()],
                            env: BTreeMap::from([("MIX_ENV".to_string(), "dev".to_string())]),
                        }),
                        initialization_options: json!({ "dialyzerEnabled": false }),
                        settings: json!({ "elixirLS": { "fetchDeps": false } }),
                    },
                )]),
            }
        );
    }

    #[test]
    fn keys_this_tier_does_not_read_are_ignored() {
        // Given Zed settings carrying a theme, and a server entry with fields tddy does not read
        let text = json!({
            "theme": "One Dark",
            "lsp": { "taplo": { "binary": { "path": "/usr/bin/taplo", "ignore_system_version": true } } }
        })
        .to_string();

        // When it is parsed
        let settings = ExtensionSettings::parse(&text).expect("unknown keys are accepted");

        // Then the server's path is read and nothing else is invented
        assert_eq!(
            settings
                .server("taplo")
                .and_then(|server| server.binary.clone())
                .and_then(|binary| binary.path),
            Some("/usr/bin/taplo".to_string())
        );
    }

    #[test]
    fn no_settings_file_is_no_settings() {
        // Given an extensions dir with no settings.json
        let (_data, dir) = an_extensions_dir();

        // When its settings are loaded
        let settings = ExtensionSettings::load(&dir);

        // Then there are none, and that is not an error
        assert_eq!(settings, Ok(ExtensionSettings::default()));
    }

    #[test]
    fn a_settings_file_that_does_not_parse_names_itself() {
        // Given a settings.json that is not JSON
        let (_data, dir) = an_extensions_dir();
        std::fs::write(ExtensionSettings::file(&dir), "{ lsp: ").expect("write the settings");

        // When its settings are loaded
        let refused = ExtensionSettings::load(&dir);

        // Then the refusal names the file rather than reading it as empty
        match refused {
            Err(ExtensionError::Io { path, .. }) => {
                assert_eq!(path, ExtensionSettings::file(&dir))
            }
            other => panic!("expected the settings file to be refused, got {other:?}"),
        }
    }
}
