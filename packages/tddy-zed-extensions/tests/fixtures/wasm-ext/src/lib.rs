//! The fixture code extension `tddy-zed-extensions`' WASM host runs (`#reshape`
//! extension-wasm-command, PRD F2 a). One component, three behaviours, chosen by the language
//! server id the host asks about:
//!
//! - `fake-ls` — the command is whatever `worktree.which("fake-ls")` finds, with `--stdio`, one env
//!   var, and initialization options and a workspace configuration the tests can recognise;
//! - `downloading-ls` — calls `download_file`, as a registry extension that fetches its server
//!   does, and passes its error on;
//! - `looping-ls` — never returns, so the host's call deadline (R5) has something to stop.
//!
//! Built once, by hand (see README.md), and committed beside this source as `extension.wasm`.

use zed_extension_api::{self as zed, serde_json::json, LanguageServerId, Result};

struct FixtureExtension;

impl zed::Extension for FixtureExtension {
    fn new() -> Self {
        FixtureExtension
    }

    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        match language_server_id.as_ref() {
            "fake-ls" => {
                let command = worktree
                    .which("fake-ls")
                    .ok_or_else(|| "fake-ls is not on the worktree's PATH".to_string())?;
                Ok(zed::Command {
                    command,
                    args: vec!["--stdio".to_string()],
                    env: vec![("FIXTURE_STARTED_BY".to_string(), "wasm".to_string())],
                })
            }
            "downloading-ls" => {
                zed::download_file(
                    "https://example.invalid/downloading-ls.tar.gz",
                    "downloading-ls",
                    zed::DownloadedFileType::GzipTar,
                )?;
                Err("the fixture's download unexpectedly succeeded".to_string())
            }
            "looping-ls" => loop {
                std::hint::spin_loop();
            },
            other => Err(format!("the fixture extension declares no server {other}")),
        }
    }

    fn language_server_initialization_options(
        &mut self,
        language_server_id: &LanguageServerId,
        _worktree: &zed::Worktree,
    ) -> Result<Option<zed::serde_json::Value>> {
        Ok(Some(json!({ "startedBy": "wasm", "server": language_server_id.as_ref() })))
    }

    fn language_server_workspace_configuration(
        &mut self,
        _language_server_id: &LanguageServerId,
        _worktree: &zed::Worktree,
    ) -> Result<Option<zed::serde_json::Value>> {
        Ok(Some(json!({ "fixture": { "mode": "wasm" } })))
    }
}

zed::register_extension!(FixtureExtension);
