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
