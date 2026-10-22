//! The running servers of the installed languages an apply reaches (`#reshape`
//! extension-language-server, R6–R7).
//!
//! A run is handed the client of the built-in server it was started for. An installed language
//! whose support launches a server of its own needs that server's client too, or its operations
//! reach nothing but the outline: so the front end that starts servers — the index daemon, or a
//! cold `restructure apply` — starts one per language [`installed_launches_for`] names, and hands
//! the clients over as [`InstalledServers`]. The registry a run routes by then builds each installed
//! support's backend over its own server.

use std::path::Path;
use std::sync::Arc;

use tddy_lsp::client::LspClient;
use tddy_lsp::{LanguageId, LaunchSpec};

use crate::language::{LiveServer, ServerHandle, SupportRegistry};
use crate::registry::BackendRegistry;
use crate::Plan;

/// The installed supports a run routes the files the built-ins do not claim to, with the clients of
/// the servers already started for their languages.
///
/// Empty by default: no installed language, which is what a run knew before extensions existed.
#[derive(Clone, Default)]
pub struct InstalledServers {
    supports: Arc<SupportRegistry>,
    clients: Vec<(LanguageId, Arc<LspClient>)>,
}

impl InstalledServers {
    /// `supports`, with no server started for any of them yet.
    pub fn over(supports: Arc<SupportRegistry>) -> Self {
        Self {
            supports,
            clients: Vec::new(),
        }
    }

    /// These, with `client` as the running server of `language`.
    pub fn with_client(mut self, language: LanguageId, client: Arc<LspClient>) -> Self {
        self.clients.push((language, client));
        self
    }

    /// The client of the server started for `language`, if one was.
    fn client_of(&self, language: &LanguageId) -> Option<&Arc<LspClient>> {
        self.clients
            .iter()
            .find(|(served, _)| served == language)
            .map(|(_, client)| client)
    }

    /// Each installed support's backend for the workspace at `root`: over its own server when one
    /// was started for its language — waiting and reporting as `built_in` does — and with none
    /// otherwise, which answers item anchors alone or refuses in the support's words.
    pub(crate) fn backends<'s>(
        &'s self,
        root: &Path,
        built_in: &LiveServer<'s>,
    ) -> BackendRegistry {
        self.supports
            .backends_over(root, |language| match self.client_of(language) {
                Some(client) => ServerHandle::live(LiveServer {
                    client,
                    ..*built_in
                }),
                None => ServerHandle::none(),
            })
    }
}

/// The languages of `installed` that launch a server and claim a file `plan` names, each once with
/// its launch, in the order the plan first names them.
///
/// A file a built-in claims is the built-in's, as it is everywhere else (K11 R4), so it names no
/// installed language here.
pub fn installed_launches_for(
    plan: &Plan,
    installed: &SupportRegistry,
) -> Vec<(LanguageId, LaunchSpec)> {
    let built_ins = SupportRegistry::with_builtins();
    let mut launches: Vec<(LanguageId, LaunchSpec)> = Vec::new();
    for op in &plan.ops {
        let file = Path::new(op.anchor.file());
        if built_ins.support_of(file).is_some() {
            continue;
        }
        let Some(support) = installed.support_of(file) else {
            continue;
        };
        if launches
            .iter()
            .any(|(language, _)| language == support.id())
        {
            continue;
        }
        if let Some(launch) = support.launch() {
            launches.push((support.id().clone(), launch));
        }
    }
    launches
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tddy_lsp::LspError;

    use super::*;
    use crate::language::LanguageSupport;
    use crate::registry::LanguageBackend;

    /// An installed language claiming files by extension, launching `launch` when it has one.
    struct AnInstalledLanguage {
        id: LanguageId,
        extension: &'static str,
        launch: Option<LaunchSpec>,
    }

    impl LanguageSupport for AnInstalledLanguage {
        fn id(&self) -> &LanguageId {
            &self.id
        }

        fn claims(&self, path: &Path) -> bool {
            path.extension().is_some_and(|ext| ext == self.extension)
        }

        fn launch(&self) -> Option<LaunchSpec> {
            self.launch.clone()
        }

        fn workspace_root(&self, _file: &Path, root: &Path) -> Result<PathBuf, LspError> {
            Ok(root.to_path_buf())
        }

        fn backend(&self, _server: ServerHandle<'_>, _root: &Path) -> Box<dyn LanguageBackend> {
            unreachable!("no launch test builds a backend")
        }
    }

    fn installed(languages: Vec<AnInstalledLanguage>) -> SupportRegistry {
        let mut registry = SupportRegistry::new();
        for language in languages {
            registry.register(Box::new(language));
        }
        registry
    }

    fn served(id: &str, extension: &'static str) -> AnInstalledLanguage {
        AnInstalledLanguage {
            id: LanguageId::new(id),
            extension,
            launch: Some(LaunchSpec::new(format!("{id}-server"))),
        }
    }

    fn a_plan_of_renames_in(files: &[&str]) -> Plan {
        let ops: Vec<String> = files
            .iter()
            .map(|file| {
                format!(
                    "{{\"op\":\"rename_symbol\",\"anchor\":{{\"kind\":\"symbol\",\"file\":\"{file}\",\
                     \"path\":\"x\"}},\"name\":\"y\"}}"
                )
            })
            .collect();
        Plan::parse(&format!(
            "{{\"v\":1,\"snapshot\":{{}}}}\n{}\n",
            ops.join("\n")
        ))
        .expect("the plan parses")
    }

    fn the_languages_launched_for(plan: &Plan, registry: &SupportRegistry) -> Vec<String> {
        installed_launches_for(plan, registry)
            .into_iter()
            .map(|(language, launch)| format!("{language}:{}", launch.program))
            .collect()
    }

    #[test]
    fn each_served_language_a_plan_names_is_launched_once_in_plan_order() {
        let registry = installed(vec![served("toml", "toml"), served("yaml", "yaml")]);
        let plan = a_plan_of_renames_in(&["b.yaml", "a.toml", "c.yaml"]);

        assert_eq!(
            the_languages_launched_for(&plan, &registry),
            vec!["yaml:yaml-server", "toml:toml-server"]
        );
    }

    #[test]
    fn a_language_without_a_server_and_a_built_in_file_launch_nothing() {
        let outline_only = AnInstalledLanguage {
            launch: None,
            ..served("toml", "toml")
        };
        let claims_rust_too = served("shadow", "rs");
        let registry = installed(vec![outline_only, claims_rust_too]);
        let plan = a_plan_of_renames_in(&["a.toml", "src/lib.rs", "notes.txt"]);

        assert_eq!(
            the_languages_launched_for(&plan, &registry),
            Vec::<String>::new()
        );
    }
}
