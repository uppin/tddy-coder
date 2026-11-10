//! Where the `codex` binary a Codex backend runs is found. The `codex-acp` binary is found beside it
//! by its tool definition (`codex-acp.yaml`'s `binary.sibling_of`), see [`crate::acp_from_spec`].

use std::path::{Path, PathBuf};

use tddy_core::CODEX_DEFAULT_CLI_BINARY;

/// Resolve Codex CLI: `--codex-cli-path` / config, then `TDDY_CODEX_CLI`, then `codex` on `PATH`.
pub(crate) fn resolve_codex_binary(codex_cli_path: Option<&Path>) -> PathBuf {
    codex_cli_path
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("TDDY_CODEX_CLI").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from(CODEX_DEFAULT_CLI_BINARY))
}
