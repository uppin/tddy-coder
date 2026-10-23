//! What the host answers an extension's imports with (R4).
//!
//! **Answered:** the worktree's `id`, `root-path`, `read-text-file` (inside the root only), `which`
//! (over the worktree's `shell-env` `PATH`) and `shell-env`; `get-settings` (`lsp` from
//! [`ExtensionSettings`], K22's file); `current-platform`.
//!
//! **Refused** with [`super::download_refusal`] for the server whose command is being computed:
//! `download-file`, `latest-github-release`, `github-release-by-tag-name`, `npm-*`,
//! `node-binary-path`, `make-file-executable`; and, under the changeset's Boundaries (no network,
//! no process spawning from WASM), `http-client.fetch` / `fetch-stream` and `process.run-command`.
//! `set-language-server-installation-status` is accepted and ignored — it reports, it installs
//! nothing.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use serde_json::json;
use wasmtime::component::{Resource, ResourceTable};

use crate::settings::ExtensionSettings;
use crate::wasm::host::bindings;
use crate::wasm::host::bindings::zed::extension::{
    common, context_server, dap, github, http_client, lsp, nodejs, platform, process, slash_command,
};
use crate::wasm::{download_refusal, WasmWorktree};

/// What a refusal names when an extension asks for a download outside a language-server call,
/// from `init-extension`.
const UNNAMED_SERVER: &str = "<server>";

/// The `get-settings` category Zed answers for a language server, keyed by the server's id.
const LANGUAGE_SERVERS_CATEGORY: &str = "language_servers";

/// One extension's store data.
pub struct HostState {
    /// The extension's id, for traps.
    pub(crate) extension: String,
    /// The server whose command, options or configuration is being computed: what a refusing
    /// import names. `None` between calls.
    pub(crate) server: Option<String>,
    /// K22's settings, which `get-settings` answers from.
    pub(crate) settings: ExtensionSettings,
    /// The worktree resources handed to the current call.
    pub(crate) table: ResourceTable,
}

impl HostState {
    pub(crate) fn new(extension: &str, settings: ExtensionSettings) -> Self {
        Self {
            extension: extension.to_string(),
            server: None,
            settings,
            table: ResourceTable::new(),
        }
    }

    /// [`download_refusal`] for the server being computed.
    fn refusal(&self) -> String {
        download_refusal(self.server.as_deref().unwrap_or(UNNAMED_SERVER))
    }

    fn worktree(&self, worktree: &Resource<WasmWorktree>) -> wasmtime::Result<&WasmWorktree> {
        Ok(self.table.get(worktree)?)
    }

    /// An import this host never hands the extension the means to reach.
    fn unreachable_import(&self, import: &str) -> wasmtime::Error {
        anyhow::anyhow!(
            "extension {} called {import}, which no export this host calls gives it a handle for",
            self.extension
        )
    }
}

impl bindings::ExtensionImports for HostState {
    /// `language_servers` with a server id as the key: that server's `lsp.<id>`, in the shape of
    /// Zed's `LspSettings`. Any other category is an `Err`, as in Zed.
    fn get_settings(
        &mut self,
        path: Option<bindings::SettingsLocation>,
        category: String,
        key: Option<String>,
    ) -> wasmtime::Result<Result<String, String>> {
        // One settings file serves every worktree, so the location does not select anything.
        let _ = path;
        if category != LANGUAGE_SERVERS_CATEGORY {
            return Ok(Err(format!("unknown settings category: {category}")));
        }
        let Some(server) = key else {
            return Ok(Err(
                "a language server's settings need the server's id as the key".to_string(),
            ));
        };
        let settings = self.settings.server(&server).cloned().unwrap_or_default();
        let binary = settings.binary.map(|binary| {
            json!({ "path": binary.path, "arguments": binary.arguments, "env": binary.env })
        });
        Ok(Ok(json!({
            "binary": binary,
            "initialization_options": settings.initialization_options,
            "settings": settings.settings,
        })
        .to_string()))
    }

    /// Refused (R4).
    fn download_file(
        &mut self,
        url: String,
        file_path: String,
        file_type: bindings::DownloadedFileType,
    ) -> wasmtime::Result<Result<(), String>> {
        let _ = (url, file_path, file_type);
        Ok(Err(self.refusal()))
    }

    /// Refused (R4).
    fn make_file_executable(&mut self, filepath: String) -> wasmtime::Result<Result<(), String>> {
        let _ = filepath;
        Ok(Err(self.refusal()))
    }

    /// Accepted and ignored: a status report installs nothing.
    fn set_language_server_installation_status(
        &mut self,
        language_server_name: String,
        status: bindings::LanguageServerInstallationStatus,
    ) -> wasmtime::Result<()> {
        let _ = (language_server_name, status);
        Ok(())
    }
}

impl bindings::HostWorktree for HostState {
    fn id(&mut self, worktree: Resource<WasmWorktree>) -> wasmtime::Result<u64> {
        Ok(self.worktree(&worktree)?.id)
    }

    fn root_path(&mut self, worktree: Resource<WasmWorktree>) -> wasmtime::Result<String> {
        Ok(self.worktree(&worktree)?.root.display().to_string())
    }

    /// Inside the root only; a path that leaves it is an `Err`.
    fn read_text_file(
        &mut self,
        worktree: Resource<WasmWorktree>,
        path: String,
    ) -> wasmtime::Result<Result<String, String>> {
        let root = &self.worktree(&worktree)?.root;
        Ok(read_inside(root, &path))
    }

    /// Searches the worktree's `shell_env` `PATH`, never this process's.
    fn which(
        &mut self,
        worktree: Resource<WasmWorktree>,
        binary_name: String,
    ) -> wasmtime::Result<Option<String>> {
        let worktree = self.worktree(&worktree)?;
        let path = worktree
            .shell_env
            .iter()
            .find(|(name, _)| name == "PATH")
            .map(|(_, value)| value);
        // A name with a separator is a path, not a binary name to search for.
        if binary_name.is_empty() || binary_name.contains('/') {
            return Ok(None);
        }
        Ok(path.and_then(|path| {
            std::env::split_paths(path)
                .map(|dir| dir.join(&binary_name))
                .find(|candidate| is_executable_file(candidate))
                .map(|found| found.display().to_string())
        }))
    }

    fn shell_env(&mut self, worktree: Resource<WasmWorktree>) -> wasmtime::Result<common::EnvVars> {
        Ok(self.worktree(&worktree)?.shell_env.clone())
    }

    fn drop(&mut self, worktree: Resource<WasmWorktree>) -> wasmtime::Result<()> {
        self.table.delete(worktree)?;
        Ok(())
    }
}

/// The text of `path` under `root`, when it stays inside `root` once symlinks and `..` are
/// resolved.
fn read_inside(root: &Path, path: &str) -> Result<String, String> {
    let root = fs::canonicalize(root).map_err(|error| {
        format!(
            "the worktree root {} is unreadable: {error}",
            root.display()
        )
    })?;
    let file = fs::canonicalize(root.join(path)).map_err(|error| format!("{path}: {error}"))?;
    if !file.starts_with(&root) {
        return Err(format!("{path} is outside the worktree"));
    }
    fs::read_to_string(&file).map_err(|error| format!("{path}: {error}"))
}

fn is_executable_file(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

/// No export this host calls hands the extension a project; never reached.
impl bindings::HostProject for HostState {
    fn worktree_ids(&mut self, project: Resource<bindings::Project>) -> wasmtime::Result<Vec<u64>> {
        let _ = project;
        Err(self.unreachable_import("project.worktree-ids"))
    }

    fn drop(&mut self, project: Resource<bindings::Project>) -> wasmtime::Result<()> {
        let _ = project;
        Ok(())
    }
}

/// No export this host calls hands the extension a key-value store; never reached.
impl bindings::HostKeyValueStore for HostState {
    fn insert(
        &mut self,
        store: Resource<bindings::KeyValueStore>,
        key: String,
        value: String,
    ) -> wasmtime::Result<Result<(), String>> {
        let _ = (store, key, value);
        Err(self.unreachable_import("key-value-store.insert"))
    }

    fn drop(&mut self, store: Resource<bindings::KeyValueStore>) -> wasmtime::Result<()> {
        let _ = store;
        Ok(())
    }
}

impl common::Host for HostState {}
impl context_server::Host for HostState {}
impl lsp::Host for HostState {}
impl slash_command::Host for HostState {}

/// No debugger: nothing here starts a debug adapter.
impl dap::Host for HostState {
    fn resolve_tcp_template(
        &mut self,
        template: dap::TcpArgumentsTemplate,
    ) -> wasmtime::Result<Result<dap::TcpArguments, String>> {
        let _ = template;
        Ok(Err("tddy does not run debug adapters".to_string()))
    }
}

/// Refused (R4).
impl github::Host for HostState {
    fn latest_github_release(
        &mut self,
        repo: String,
        options: github::GithubReleaseOptions,
    ) -> wasmtime::Result<Result<github::GithubRelease, String>> {
        let _ = (repo, options);
        Ok(Err(self.refusal()))
    }

    fn github_release_by_tag_name(
        &mut self,
        repo: String,
        tag: String,
    ) -> wasmtime::Result<Result<github::GithubRelease, String>> {
        let _ = (repo, tag);
        Ok(Err(self.refusal()))
    }
}

/// Refused: no network from WASM (Boundaries).
impl http_client::Host for HostState {
    fn fetch(
        &mut self,
        req: http_client::HttpRequest,
    ) -> wasmtime::Result<Result<http_client::HttpResponse, String>> {
        let _ = req;
        Ok(Err(self.refusal()))
    }

    fn fetch_stream(
        &mut self,
        req: http_client::HttpRequest,
    ) -> wasmtime::Result<Result<Resource<http_client::HttpResponseStream>, String>> {
        let _ = req;
        Ok(Err(self.refusal()))
    }
}

/// Never handed out: [`http_client::Host::fetch_stream`] is refused.
impl http_client::HostHttpResponseStream for HostState {
    fn next_chunk(
        &mut self,
        stream: Resource<http_client::HttpResponseStream>,
    ) -> wasmtime::Result<Result<Option<Vec<u8>>, String>> {
        let _ = stream;
        Err(self.unreachable_import("http-response-stream.next-chunk"))
    }

    fn drop(&mut self, stream: Resource<http_client::HttpResponseStream>) -> wasmtime::Result<()> {
        let _ = stream;
        Ok(())
    }
}

/// Refused (R4).
impl nodejs::Host for HostState {
    fn node_binary_path(&mut self) -> wasmtime::Result<Result<String, String>> {
        Ok(Err(self.refusal()))
    }

    fn npm_package_latest_version(
        &mut self,
        package_name: String,
    ) -> wasmtime::Result<Result<String, String>> {
        let _ = package_name;
        Ok(Err(self.refusal()))
    }

    fn npm_package_installed_version(
        &mut self,
        package_name: String,
    ) -> wasmtime::Result<Result<Option<String>, String>> {
        let _ = package_name;
        Ok(Err(self.refusal()))
    }

    fn npm_install_package(
        &mut self,
        package_name: String,
        version: String,
    ) -> wasmtime::Result<Result<(), String>> {
        let _ = (package_name, version);
        Ok(Err(self.refusal()))
    }
}

impl platform::Host for HostState {
    /// This process's own platform; one WIT does not name is a trap, not a guess.
    fn current_platform(&mut self) -> wasmtime::Result<(platform::Os, platform::Architecture)> {
        let os = match std::env::consts::OS {
            "macos" => platform::Os::Mac,
            "linux" => platform::Os::Linux,
            "windows" => platform::Os::Windows,
            other => anyhow::bail!(
                "extension {}: no WIT operating system for {other}",
                self.extension
            ),
        };
        let architecture = match std::env::consts::ARCH {
            "aarch64" => platform::Architecture::Aarch64,
            "x86" => platform::Architecture::X86,
            "x86_64" => platform::Architecture::X8664,
            other => anyhow::bail!(
                "extension {}: no WIT architecture for {other}",
                self.extension
            ),
        };
        Ok((os, architecture))
    }
}

/// Refused: no process spawning from WASM (Boundaries).
impl process::Host for HostState {
    fn run_command(
        &mut self,
        command: process::Command,
    ) -> wasmtime::Result<Result<process::Output, String>> {
        let _ = command;
        Ok(Err(self.refusal()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bindings::ExtensionImports;
    use bindings::HostWorktree;
    use pretty_assertions::assert_eq;
    use std::os::unix::fs::symlink;

    /// A temp worktree root holding `notes.txt`, a `bin/` with an executable `tool` and a plain
    /// `data`, and a file outside the root reachable through a symlink `escape`.
    struct AWorktreeOnDisk {
        _outside: tempfile::TempDir,
        root: tempfile::TempDir,
    }

    impl AWorktreeOnDisk {
        fn new() -> Self {
            let root = tempfile::TempDir::new().expect("temp root");
            let outside = tempfile::TempDir::new().expect("temp outside");
            fs::write(root.path().join("notes.txt"), "inside").expect("write notes.txt");
            fs::write(outside.path().join("secret.txt"), "outside").expect("write secret.txt");
            symlink(
                outside.path().join("secret.txt"),
                root.path().join("escape"),
            )
            .expect("link escape");
            let bin = root.path().join("bin");
            fs::create_dir_all(&bin).expect("bin dir");
            fs::write(bin.join("tool"), "#!/bin/sh\n").expect("write tool");
            fs::set_permissions(bin.join("tool"), fs::Permissions::from_mode(0o755))
                .expect("make tool executable");
            fs::write(bin.join("data"), "").expect("write data");
            Self {
                _outside: outside,
                root,
            }
        }

        fn worktree(&self) -> WasmWorktree {
            WasmWorktree {
                id: 7,
                root: self.root.path().to_path_buf(),
                shell_env: vec![(
                    "PATH".to_string(),
                    self.root.path().join("bin").display().to_string(),
                )],
            }
        }
    }

    fn a_host_state() -> HostState {
        HostState::new("an-extension", ExtensionSettings::default())
    }

    #[test]
    fn read_text_file_reads_inside_the_root_and_nothing_outside_it() {
        // Given a worktree with a file inside and a symlink and a `..` path leading out of it
        let disk = AWorktreeOnDisk::new();
        let mut state = a_host_state();
        let worktree = state.table.push(disk.worktree()).expect("push");
        let rep = worktree.rep();
        let mut read = |path: &str| {
            state
                .read_text_file(Resource::new_borrow(rep), path.to_string())
                .expect("no trap")
                .map_err(|error| error.ends_with("is outside the worktree"))
        };

        // When each is read
        let reads = (read("notes.txt"), read("escape"), read("../notes.txt"));

        // Then only the file inside is read; both ways out are refused as outside
        assert_eq!(reads.0, Ok("inside".to_string()));
        assert_eq!(reads.1, Err(true));
        assert!(reads.2.is_err(), "a `..` path out of the root was read");
    }

    #[test]
    fn which_finds_only_executables_on_the_worktrees_path() {
        // Given a worktree whose PATH holds an executable `tool` and a plain file `data`
        let disk = AWorktreeOnDisk::new();
        let mut state = a_host_state();
        let rep = state.table.push(disk.worktree()).expect("push").rep();
        let mut which = |name: &str| {
            state
                .which(Resource::new_borrow(rep), name.to_string())
                .expect("no trap")
        };

        // When names are looked up
        let found = [
            which("tool"),
            which("data"),
            which("missing"),
            which("bin/tool"),
        ];

        // Then only the executable is found, and a path is never searched for
        assert_eq!(
            found,
            [
                Some(disk.root.path().join("bin/tool").display().to_string()),
                None,
                None,
                None
            ]
        );
    }

    #[test]
    fn the_worktree_answers_its_id_root_and_shell_env() {
        // Given a worktree handed to a call
        let disk = AWorktreeOnDisk::new();
        let mut state = a_host_state();
        let rep = state.table.push(disk.worktree()).expect("push").rep();

        // When the extension asks for its id, root and shell env
        let id = state.id(Resource::new_borrow(rep)).expect("id");
        let root = state.root_path(Resource::new_borrow(rep)).expect("root");
        let env = state.shell_env(Resource::new_borrow(rep)).expect("env");

        // Then each is the worktree's own
        assert_eq!((id, root, env), {
            let worktree = disk.worktree();
            (7, worktree.root.display().to_string(), worktree.shell_env)
        });
    }

    #[test]
    fn get_settings_answers_a_language_servers_lsp_block_in_zeds_shape() {
        // Given settings configuring elixir-ls
        let settings: ExtensionSettings = serde_json::from_value(json!({
            "lsp": { "elixir-ls": {
                "binary": { "path": "/opt/elixir-ls", "arguments": ["--stdio"] },
                "initialization_options": { "a": 1 },
                "settings": { "b": 2 }
            } }
        }))
        .expect("settings parse");
        let mut state = HostState::new("an-extension", settings);

        // When the extension asks for elixir-ls, for no key, and for another category
        let mut ask = |category: &str, key: Option<&str>| {
            state
                .get_settings(None, category.to_string(), key.map(str::to_string))
                .expect("no trap")
                .map(|text| serde_json::from_str::<serde_json::Value>(&text).expect("JSON"))
        };
        let answers = (
            ask(LANGUAGE_SERVERS_CATEGORY, Some("elixir-ls")),
            ask(LANGUAGE_SERVERS_CATEGORY, None).is_err(),
            ask("editor", Some("tab_size")),
        );

        // Then the server's block comes back as Zed's LspSettings; the others are errors
        assert_eq!(
            answers,
            (
                Ok(json!({
                    "binary": { "path": "/opt/elixir-ls", "arguments": ["--stdio"], "env": {} },
                    "initialization_options": { "a": 1 },
                    "settings": { "b": 2 }
                })),
                true,
                Err("unknown settings category: editor".to_string())
            )
        );
    }

    #[test]
    fn a_download_outside_a_server_call_is_refused_without_a_server_name() {
        // Given no server being computed — the extension is in init-extension
        let mut state = a_host_state();

        // When it asks to download a file
        let refused = state
            .download_file(
                "https://example.invalid/ls.tgz".to_string(),
                "ls".to_string(),
                bindings::DownloadedFileType::GzipTar,
            )
            .expect("no trap");

        // Then it is refused, naming no particular server
        assert_eq!(refused, Err(download_refusal(UNNAMED_SERVER)));
    }

    #[test]
    fn the_current_platform_is_the_one_this_test_runs_on() {
        // Given a host state, and the platform this test was built for
        let mut state = a_host_state();
        let os = match std::env::consts::OS {
            "macos" => Some(platform::Os::Mac),
            "linux" => Some(platform::Os::Linux),
            _ => None,
        };
        let architecture = match std::env::consts::ARCH {
            "aarch64" => Some(platform::Architecture::Aarch64),
            "x86_64" => Some(platform::Architecture::X8664),
            _ => None,
        };

        // When the extension asks for its platform
        let platform = platform::Host::current_platform(&mut state)
            .map(|(os, architecture)| (Some(os), Some(architecture)))
            .map_err(|error| error.to_string());

        // Then it is that platform
        assert_eq!(platform, Ok((os, architecture)));
    }
}
