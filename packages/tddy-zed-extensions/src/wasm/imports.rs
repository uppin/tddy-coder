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

use wasmtime::component::{Resource, ResourceTable};

use crate::settings::ExtensionSettings;
use crate::wasm::host::bindings;
use crate::wasm::host::bindings::zed::extension::{
    common, context_server, dap, github, http_client, lsp, nodejs, platform, process, slash_command,
};
use crate::wasm::WasmWorktree;

/// One extension's store data.
pub struct HostState {
    /// The extension's id, for refusals.
    #[allow(
        dead_code,
        reason = "TODO(reshape-extension-wasm-command): read by the refusing imports once implemented"
    )]
    pub(crate) extension: String,
    /// The server whose command, options or configuration is being computed: what a refusing
    /// import names. `None` between calls.
    #[allow(
        dead_code,
        reason = "TODO(reshape-extension-wasm-command): read by the refusing imports once implemented"
    )]
    pub(crate) server: Option<String>,
    /// K22's settings, which `get-settings` answers from.
    #[allow(
        dead_code,
        reason = "TODO(reshape-extension-wasm-command): read by get-settings once implemented"
    )]
    pub(crate) settings: ExtensionSettings,
    /// The worktree resources handed to the current call.
    #[allow(
        dead_code,
        reason = "TODO(reshape-extension-wasm-command): read by the worktree imports once implemented"
    )]
    pub(crate) table: ResourceTable,
}

impl bindings::ExtensionImports for HostState {
    fn get_settings(
        &mut self,
        path: Option<bindings::SettingsLocation>,
        category: String,
        key: Option<String>,
    ) -> wasmtime::Result<Result<String, String>> {
        let _ = (path, category, key);
        // TODO(reshape-extension-wasm-command): implement
        todo!("get-settings")
    }

    /// Refused (R4).
    fn download_file(
        &mut self,
        url: String,
        file_path: String,
        file_type: bindings::DownloadedFileType,
    ) -> wasmtime::Result<Result<(), String>> {
        let _ = (url, file_path, file_type);
        // TODO(reshape-extension-wasm-command): implement
        todo!("download-file")
    }

    /// Refused (R4).
    fn make_file_executable(&mut self, filepath: String) -> wasmtime::Result<Result<(), String>> {
        let _ = filepath;
        // TODO(reshape-extension-wasm-command): implement
        todo!("make-file-executable")
    }

    /// Accepted and ignored: a status report installs nothing.
    fn set_language_server_installation_status(
        &mut self,
        language_server_name: String,
        status: bindings::LanguageServerInstallationStatus,
    ) -> wasmtime::Result<()> {
        let _ = (language_server_name, status);
        // TODO(reshape-extension-wasm-command): implement
        todo!("set-language-server-installation-status")
    }
}

impl bindings::HostWorktree for HostState {
    fn id(&mut self, worktree: Resource<WasmWorktree>) -> wasmtime::Result<u64> {
        let _ = worktree;
        // TODO(reshape-extension-wasm-command): implement
        todo!("worktree.id")
    }

    fn root_path(&mut self, worktree: Resource<WasmWorktree>) -> wasmtime::Result<String> {
        let _ = worktree;
        // TODO(reshape-extension-wasm-command): implement
        todo!("worktree.root-path")
    }

    /// Inside the root only; a path that leaves it is an `Err`.
    fn read_text_file(
        &mut self,
        worktree: Resource<WasmWorktree>,
        path: String,
    ) -> wasmtime::Result<Result<String, String>> {
        let _ = (worktree, path);
        // TODO(reshape-extension-wasm-command): implement
        todo!("worktree.read-text-file")
    }

    /// Searches the worktree's `shell_env` `PATH`, never this process's.
    fn which(
        &mut self,
        worktree: Resource<WasmWorktree>,
        binary_name: String,
    ) -> wasmtime::Result<Option<String>> {
        let _ = (worktree, binary_name);
        // TODO(reshape-extension-wasm-command): implement
        todo!("worktree.which")
    }

    fn shell_env(&mut self, worktree: Resource<WasmWorktree>) -> wasmtime::Result<common::EnvVars> {
        let _ = worktree;
        // TODO(reshape-extension-wasm-command): implement
        todo!("worktree.shell-env")
    }

    fn drop(&mut self, worktree: Resource<WasmWorktree>) -> wasmtime::Result<()> {
        let _ = worktree;
        // TODO(reshape-extension-wasm-command): implement
        todo!("worktree drop")
    }
}

/// No export this host calls hands the extension a project; never reached.
impl bindings::HostProject for HostState {
    fn worktree_ids(&mut self, project: Resource<bindings::Project>) -> wasmtime::Result<Vec<u64>> {
        let _ = project;
        // TODO(reshape-extension-wasm-command): implement
        todo!("project.worktree-ids")
    }

    fn drop(&mut self, project: Resource<bindings::Project>) -> wasmtime::Result<()> {
        let _ = project;
        // TODO(reshape-extension-wasm-command): implement
        todo!("project drop")
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
        // TODO(reshape-extension-wasm-command): implement
        todo!("key-value-store.insert")
    }

    fn drop(&mut self, store: Resource<bindings::KeyValueStore>) -> wasmtime::Result<()> {
        let _ = store;
        // TODO(reshape-extension-wasm-command): implement
        todo!("key-value-store drop")
    }
}

impl common::Host for HostState {}
impl context_server::Host for HostState {}
impl lsp::Host for HostState {}
impl slash_command::Host for HostState {}

impl dap::Host for HostState {
    fn resolve_tcp_template(
        &mut self,
        template: dap::TcpArgumentsTemplate,
    ) -> wasmtime::Result<Result<dap::TcpArguments, String>> {
        let _ = template;
        // TODO(reshape-extension-wasm-command): implement
        todo!("dap.resolve-tcp-template")
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
        // TODO(reshape-extension-wasm-command): implement
        todo!("github.latest-github-release")
    }

    fn github_release_by_tag_name(
        &mut self,
        repo: String,
        tag: String,
    ) -> wasmtime::Result<Result<github::GithubRelease, String>> {
        let _ = (repo, tag);
        // TODO(reshape-extension-wasm-command): implement
        todo!("github.github-release-by-tag-name")
    }
}

/// Refused: no network from WASM (Boundaries).
impl http_client::Host for HostState {
    fn fetch(
        &mut self,
        req: http_client::HttpRequest,
    ) -> wasmtime::Result<Result<http_client::HttpResponse, String>> {
        let _ = req;
        // TODO(reshape-extension-wasm-command): implement
        todo!("http-client.fetch")
    }

    fn fetch_stream(
        &mut self,
        req: http_client::HttpRequest,
    ) -> wasmtime::Result<Result<Resource<http_client::HttpResponseStream>, String>> {
        let _ = req;
        // TODO(reshape-extension-wasm-command): implement
        todo!("http-client.fetch-stream")
    }
}

/// Never handed out: [`http_client::Host::fetch_stream`] is refused.
impl http_client::HostHttpResponseStream for HostState {
    fn next_chunk(
        &mut self,
        stream: Resource<http_client::HttpResponseStream>,
    ) -> wasmtime::Result<Result<Option<Vec<u8>>, String>> {
        let _ = stream;
        // TODO(reshape-extension-wasm-command): implement
        todo!("http-response-stream.next-chunk")
    }

    fn drop(&mut self, stream: Resource<http_client::HttpResponseStream>) -> wasmtime::Result<()> {
        let _ = stream;
        // TODO(reshape-extension-wasm-command): implement
        todo!("http-response-stream drop")
    }
}

/// Refused (R4).
impl nodejs::Host for HostState {
    fn node_binary_path(&mut self) -> wasmtime::Result<Result<String, String>> {
        // TODO(reshape-extension-wasm-command): implement
        todo!("nodejs.node-binary-path")
    }

    fn npm_package_latest_version(
        &mut self,
        package_name: String,
    ) -> wasmtime::Result<Result<String, String>> {
        let _ = package_name;
        // TODO(reshape-extension-wasm-command): implement
        todo!("nodejs.npm-package-latest-version")
    }

    fn npm_package_installed_version(
        &mut self,
        package_name: String,
    ) -> wasmtime::Result<Result<Option<String>, String>> {
        let _ = package_name;
        // TODO(reshape-extension-wasm-command): implement
        todo!("nodejs.npm-package-installed-version")
    }

    fn npm_install_package(
        &mut self,
        package_name: String,
        version: String,
    ) -> wasmtime::Result<Result<(), String>> {
        let _ = (package_name, version);
        // TODO(reshape-extension-wasm-command): implement
        todo!("nodejs.npm-install-package")
    }
}

impl platform::Host for HostState {
    fn current_platform(&mut self) -> wasmtime::Result<(platform::Os, platform::Architecture)> {
        // TODO(reshape-extension-wasm-command): implement
        todo!("platform.current-platform")
    }
}

/// Refused: no process spawning from WASM (Boundaries).
impl process::Host for HostState {
    fn run_command(
        &mut self,
        command: process::Command,
    ) -> wasmtime::Result<Result<process::Output, String>> {
        let _ = command;
        // TODO(reshape-extension-wasm-command): implement
        todo!("process.run-command")
    }
}
