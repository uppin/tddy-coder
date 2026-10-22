//! Which language server serves an extension language, and the command it starts with
//! (`#reshape` extension-language-server, R1–R4).
//!
//! The manifest's `[language_servers.<id>]` entries say **which** server serves which of the
//! extension's languages; Zed takes the **command** from the extension's WASM unless the user set
//! `lsp.<id>.binary.path`. tddy runs no WASM here, so that setting is the only command there is,
//! and a declared server without it is refused by name — never looked up on `PATH` (PRD F1 a).

use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use tddy_lsp::LaunchSpec;

use crate::dir::ExtensionsDir;
use crate::error::ExtensionError;
use crate::language::ExtensionLanguage;
use crate::manifest::{ExtensionManifest, LanguageServerEntry};
use crate::settings::ExtensionSettings;

/// A declared server whose command is not configured (R3), in the words the PRD gives.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "language server {server} (extension {extension}) has no command: set lsp.{server}.binary.path in {}",
    settings_file.display()
)]
pub struct NoServerCommand {
    /// The `[language_servers.<id>]` id.
    pub server: String,
    /// The extension that declares it.
    pub extension: String,
    /// `<extensions dir>/settings.json`, where the setting belongs.
    pub settings_file: PathBuf,
}

/// A declared server that cannot be started for a reason other than a missing command — a code
/// extension (`extension-wasm-command`, K24) refused it: its WASM asked for a download, or exceeded
/// its deadline. Shown as `reason` alone, the extension's own words (developer decision 2026-10-20).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{reason}")]
pub struct ServerRefusal {
    /// The `[language_servers.<id>]` id.
    pub server: String,
    /// Why it cannot be started, as the run reports it.
    pub reason: String,
}

/// What serves one extension language.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ServerChoice {
    /// No `[language_servers.<id>]` names the language: an outline-only language, as
    /// `extension-outline` left it.
    #[default]
    Undeclared,
    /// A server is declared and has no `lsp.<id>.binary.path`: every server operation is refused
    /// with this, before any process starts (R3).
    Unconfigured(NoServerCommand),
    /// A server is declared and cannot be started for another reason — a code extension (K24)
    /// refused it: every server operation is refused with exactly [`ServerRefusal::reason`],
    /// before any process starts.
    Refused(ServerRefusal),
    /// A server is declared and configured.
    Configured {
        /// The `[language_servers.<id>]` id.
        server: String,
        /// Program, arguments and env from `lsp.<id>.binary`; `initialization_options`;
        /// `settings` as its `workspace_configuration`; [`client_capabilities`]; and the
        /// manifest's `language_ids` entry for the language as its `language_id`, empty when it
        /// has none (the allow-list then opens documents as the support's id).
        launch: LaunchSpec,
    },
}

impl ServerChoice {
    /// R1–R4 for `language`, an extension language `manifest` declares, from `settings` read at
    /// `settings_file`.
    ///
    /// R1: a `[language_servers.<id>]` whose `language` or `languages` names `language.name` —
    /// unless the entry's `opt_in_languages` lists the language too, in which case it serves it
    /// only when `settings` names it there ([`ExtensionSettings::names_server`]:
    /// `languages.<Name>.language_servers` lists the bare id); an opt-in language no setting names
    /// is [`ServerChoice::Undeclared`] for that server. Which of several qualifying servers serves
    /// it is still open (see the changeset).
    ///
    /// The launch's `language_id` is the entry's `language_ids` value for `language.name` when it
    /// has one; otherwise it is left empty, and the allow-list opens documents as the support's id
    /// (`language-support`'s default).
    pub fn for_language(
        manifest: &ExtensionManifest,
        language: &ExtensionLanguage,
        settings: &ExtensionSettings,
        settings_file: &Path,
    ) -> Self {
        // The multi-server tie-break is open: the first qualifying server, by id, serves it.
        let Some((id, entry)) = manifest
            .language_servers
            .iter()
            .find(|(id, entry)| serves(entry, id, language, settings))
        else {
            return ServerChoice::Undeclared;
        };
        let configured = settings.server(id).and_then(|server| {
            let binary = server.binary.as_ref()?;
            Some((server, binary, binary.path.as_ref()?))
        });
        let Some((server, binary, path)) = configured else {
            return ServerChoice::Unconfigured(NoServerCommand {
                server: id.clone(),
                extension: manifest.id.clone(),
                settings_file: settings_file.to_path_buf(),
            });
        };
        let mut launch = LaunchSpec::new(path)
            .with_capabilities(client_capabilities())
            .with_initialization_options(server.initialization_options.clone())
            .with_workspace_configuration(server.settings.clone())
            .with_language_id(
                entry
                    .language_ids
                    .get(&language.name)
                    .cloned()
                    .unwrap_or_default(),
            );
        launch.args = binary.arguments.clone();
        launch.env = binary
            .env
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        ServerChoice::Configured {
            server: id.clone(),
            launch,
        }
    }

    /// The launch of a configured server.
    pub fn launch(&self) -> Option<&LaunchSpec> {
        match self {
            ServerChoice::Configured { launch, .. } => Some(launch),
            ServerChoice::Undeclared | ServerChoice::Unconfigured(_) | ServerChoice::Refused(_) => {
                None
            }
        }
    }

    /// The words every server operation of the language is refused with — the reason its support
    /// builds a [`tddy_code_restructuring::backends::ServerBackend::without_command`] with:
    /// [`NoServerCommand`]'s text for [`ServerChoice::Unconfigured`], exactly
    /// [`ServerRefusal::reason`] for [`ServerChoice::Refused`]; `None` when there is nothing to
    /// refuse ([`ServerChoice::Undeclared`], [`ServerChoice::Configured`]).
    pub fn refusal(&self) -> Option<String> {
        match self {
            ServerChoice::Unconfigured(no_command) => Some(no_command.to_string()),
            ServerChoice::Refused(refusal) => Some(refusal.reason.clone()),
            ServerChoice::Undeclared | ServerChoice::Configured { .. } => None,
        }
    }
}

/// Each of `languages` — an installed extension's, beside its manifest — with what serves it, from
/// the settings under `dir`, read once ([`ExtensionSettings::load`]).
pub(crate) fn served(
    dir: &ExtensionsDir,
    languages: Vec<(ExtensionManifest, ExtensionLanguage)>,
) -> Result<Vec<(ExtensionLanguage, ServerChoice)>, ExtensionError> {
    let settings = ExtensionSettings::load(dir)?;
    let settings_file = ExtensionSettings::file(dir);
    Ok(languages
        .into_iter()
        .map(|(manifest, language)| {
            let server =
                ServerChoice::for_language(&manifest, &language, &settings, &settings_file);
            (language, server)
        })
        .collect())
}

/// What tddy advertises to an extension language's server at `initialize`: byte (`utf-8`)
/// positions, which every coordinate here is counted in, and `rename` support.
pub fn client_capabilities() -> Value {
    json!({
        "general": { "positionEncodings": ["utf-8"] },
        "textDocument": { "rename": { "prepareSupport": false } },
    })
}

/// Whether the manifest entry `id` serves `language`: it names the language in `language` or
/// `languages`, and, when it lists the language as opt-in, `settings` names it there (R1).
fn serves(
    entry: &LanguageServerEntry,
    id: &str,
    language: &ExtensionLanguage,
    settings: &ExtensionSettings,
) -> bool {
    let declared = entry.language.as_deref() == Some(language.name.as_str())
        || entry.languages.contains(&language.name);
    declared
        && (!entry.opt_in_languages.contains(&language.name)
            || settings.names_server(&language.name, id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{BinarySettings, LanguageSettings, ServerSettings};
    use pretty_assertions::assert_eq;
    use serde_json::json;
    use std::collections::BTreeMap;

    const SETTINGS_FILE: &str = "/data/extensions/settings.json";

    /// The `Demo TOML` language of the `demo-toml` extension.
    fn demo_toml() -> ExtensionLanguage {
        ExtensionLanguage {
            extension: "demo-toml".into(),
            extension_dir: PathBuf::from("/data/extensions/installed/demo-toml"),
            name: "Demo TOML".into(),
            grammar: "toml".into(),
            path_suffixes: vec!["toml".into()],
            outline_query: None,
        }
    }

    /// The demo-toml manifest with `servers` appended — each a `[language_servers.<id>]` table.
    fn a_manifest_declaring(servers: &str) -> ExtensionManifest {
        ExtensionManifest::parse(&format!(
            "id = \"demo-toml\"\nname = \"Demo TOML\"\nversion = \"0.1.0\"\n{servers}"
        ))
        .expect("the manifest parses")
    }

    fn settings_for(server: &str, settings: ServerSettings) -> ExtensionSettings {
        ExtensionSettings {
            lsp: BTreeMap::from([(server.to_string(), settings)]),
            ..ExtensionSettings::default()
        }
    }

    /// Settings running fake-ls from `/opt/fake-ls`, with nothing else.
    fn fake_ls_configured() -> ExtensionSettings {
        settings_for(
            "fake-ls",
            ServerSettings {
                binary: Some(BinarySettings {
                    path: Some("/opt/fake-ls".into()),
                    ..BinarySettings::default()
                }),
                ..ServerSettings::default()
            },
        )
    }

    /// `settings` with `languages."Demo TOML".language_servers` set to `servers`.
    fn naming_for_demo_toml(
        mut settings: ExtensionSettings,
        servers: &[&str],
    ) -> ExtensionSettings {
        settings.languages.insert(
            "Demo TOML".into(),
            LanguageSettings {
                language_servers: servers.iter().map(|id| id.to_string()).collect(),
            },
        );
        settings
    }

    fn the_choice(manifest: &ExtensionManifest, settings: &ExtensionSettings) -> ServerChoice {
        ServerChoice::for_language(manifest, &demo_toml(), settings, Path::new(SETTINGS_FILE))
    }

    #[test]
    fn a_configured_server_launches_its_binary_with_its_settings() {
        // Given fake-ls declared for Demo TOML, and settings giving it a binary and options
        let manifest =
            a_manifest_declaring("[language_servers.fake-ls]\nlanguages = [\"Demo TOML\"]\n");
        let settings = settings_for(
            "fake-ls",
            ServerSettings {
                binary: Some(BinarySettings {
                    path: Some("/opt/fake-ls".into()),
                    arguments: vec!["--stdio".into()],
                    env: BTreeMap::from([("FAKE_LOG".into(), "debug".into())]),
                }),
                initialization_options: json!({ "formatter": { "indent": 2 } }),
                settings: json!({ "fake": { "mode": "strict" } }),
            },
        );

        // When the language's server is chosen
        let choice = the_choice(&manifest, &settings);

        // Then it launches the binary as written, carrying both kinds of settings (R2, R4)
        let mut launch = LaunchSpec::new("/opt/fake-ls")
            .with_capabilities(client_capabilities())
            .with_initialization_options(json!({ "formatter": { "indent": 2 } }))
            .with_workspace_configuration(json!({ "fake": { "mode": "strict" } }));
        launch.args = vec!["--stdio".into()];
        launch.env = vec![("FAKE_LOG".into(), "debug".into())];
        assert_eq!(
            choice,
            ServerChoice::Configured {
                server: "fake-ls".into(),
                launch,
            }
        );
    }

    #[test]
    fn a_declared_server_without_a_binary_path_is_refused_by_name() {
        // Given fake-ls declared for Demo TOML (singular `language`), with options but no binary
        let manifest =
            a_manifest_declaring("[language_servers.fake-ls]\nlanguage = \"Demo TOML\"\n");
        let settings = settings_for(
            "fake-ls",
            ServerSettings {
                initialization_options: json!({ "formatter": {} }),
                ..ServerSettings::default()
            },
        );

        // When the language's server is chosen
        let choice = the_choice(&manifest, &settings);

        // Then it is unconfigured, naming server, extension and the file the setting goes in —
        // no command is guessed (R2, R3)
        assert_eq!(
            (choice.launch(), choice.clone()),
            (
                None,
                ServerChoice::Unconfigured(NoServerCommand {
                    server: "fake-ls".into(),
                    extension: "demo-toml".into(),
                    settings_file: PathBuf::from(SETTINGS_FILE),
                })
            )
        );
    }

    #[test]
    fn a_language_no_server_names_has_no_server() {
        // Given a server declared for another of the extension's languages, and settings for it
        let manifest =
            a_manifest_declaring("[language_servers.other-ls]\nlanguages = [\"Other\"]\n");
        let settings = settings_for(
            "other-ls",
            ServerSettings {
                binary: Some(BinarySettings {
                    path: Some("/opt/other-ls".into()),
                    ..BinarySettings::default()
                }),
                ..ServerSettings::default()
            },
        );

        // When Demo TOML's server is chosen
        let choice = the_choice(&manifest, &settings);

        // Then it has none (R1)
        assert_eq!(choice, ServerChoice::Undeclared);
    }

    #[test]
    fn the_manifests_language_id_is_the_documents_language_id() {
        // Given fake-ls declared for Demo TOML with an LSP language id for it, and configured
        let manifest = a_manifest_declaring(
            "[language_servers.fake-ls]\nlanguages = [\"Demo TOML\"]\n\
             [language_servers.fake-ls.language_ids]\n\"Demo TOML\" = \"toml\"\n",
        );
        let settings = settings_for(
            "fake-ls",
            ServerSettings {
                binary: Some(BinarySettings {
                    path: Some("/opt/fake-ls".into()),
                    ..BinarySettings::default()
                }),
                ..ServerSettings::default()
            },
        );

        // When the language's server is chosen
        let choice = the_choice(&manifest, &settings);

        // Then documents of the language are opened under the manifest's id for it
        assert_eq!(
            choice.launch().map(|launch| launch.language_id.as_str()),
            Some("toml")
        );
    }

    #[test]
    fn without_language_ids_the_documents_language_id_is_left_to_the_support() {
        // Given fake-ls declared for Demo TOML with no `language_ids`, and configured
        let manifest =
            a_manifest_declaring("[language_servers.fake-ls]\nlanguages = [\"Demo TOML\"]\n");

        // When the language's server is chosen
        let choice = the_choice(&manifest, &fake_ls_configured());

        // Then its launch names no language id, so the allow-list's default — the support's id —
        // opens its documents
        assert_eq!(
            choice.launch().map(|launch| launch.language_id.as_str()),
            Some("")
        );
    }

    #[test]
    fn an_opt_in_language_does_not_pick_the_server() {
        // Given fake-ls declared for Demo TOML only as an opt-in language, and configured
        let manifest = a_manifest_declaring(
            "[language_servers.fake-ls]\nlanguages = [\"Demo TOML\"]\n\
             opt_in_languages = [\"Demo TOML\"]\n",
        );

        // When the language's server is chosen with settings that do not name it for Demo TOML —
        // `...` included — and with settings that do
        let unnamed = the_choice(&manifest, &fake_ls_configured());
        let rest_only = the_choice(
            &manifest,
            &naming_for_demo_toml(fake_ls_configured(), &["..."]),
        );
        let named = the_choice(
            &manifest,
            &naming_for_demo_toml(fake_ls_configured(), &["fake-ls", "..."]),
        );

        // Then it serves Demo TOML only where `languages."Demo TOML".language_servers` names it
        assert_eq!(
            (
                unnamed,
                rest_only,
                named.launch().map(|launch| launch.program.as_str())
            ),
            (
                ServerChoice::Undeclared,
                ServerChoice::Undeclared,
                Some("/opt/fake-ls")
            )
        );
    }

    #[test]
    fn a_refused_server_is_refused_with_exactly_its_reason() {
        // Given a server a code extension refused to start — its WASM asked for a download
        let reason = "extension demo-toml: language server fake-ls asked to download \
                      https://example.com/fake-ls.tar.gz; downloads are not allowed";
        let choice = ServerChoice::Refused(ServerRefusal {
            server: "fake-ls".into(),
            reason: reason.into(),
        });

        // When what its operations are refused with is asked
        let refused = (choice.launch(), choice.refusal());

        // Then it has no launch, and the refusal is the extension's words, unchanged
        assert_eq!(refused, (None, Some(reason.to_string())));
    }

    #[test]
    fn the_refusal_reads_as_the_prd_words_it() {
        // Given a declared server with no command
        let refusal = NoServerCommand {
            server: "elixir-ls".into(),
            extension: "elixir".into(),
            settings_file: PathBuf::from(SETTINGS_FILE),
        };

        // When it is shown
        let shown = refusal.to_string();

        // Then it names the server, its extension and the setting to write, and where
        assert_eq!(
            shown,
            "language server elixir-ls (extension elixir) has no command: set \
             lsp.elixir-ls.binary.path in /data/extensions/settings.json"
        );
    }
}
