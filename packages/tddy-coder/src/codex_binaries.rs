//! Where the `codex` and `codex-acp` binaries a Codex backend runs are found.

use std::path::{Path, PathBuf};

use tddy_core::CodexBackend;

/// Resolve Codex CLI: `--codex-cli-path` / config, then `TDDY_CODEX_CLI`, then `codex` on `PATH`.
pub(crate) fn resolve_codex_binary(codex_cli_path: Option<&Path>) -> PathBuf {
    codex_cli_path
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("TDDY_CODEX_CLI").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from(CodexBackend::DEFAULT_CLI_BINARY))
}

/// Resolve `codex-acp` stdio agent: explicit path / env, then `codex-acp` beside resolved `codex`, else `codex-acp` on `PATH`.
pub(crate) fn resolve_codex_acp_binary(
    codex_acp_cli_path: Option<&Path>,
    codex_cli_path: Option<&Path>,
) -> PathBuf {
    if let Some(p) = codex_acp_cli_path {
        return p.to_path_buf();
    }
    if let Some(p) = std::env::var_os("TDDY_CODEX_ACP_CLI").map(PathBuf::from) {
        return p;
    }
    let codex = resolve_codex_binary(codex_cli_path);
    if let Some(acp) = codex_acp_beside_resolved_codex(&codex) {
        return acp;
    }
    PathBuf::from(tddy_core::backend::codex_acp::DEFAULT_CODEX_ACP_BINARY)
}

/// If `codex` resolves to a concrete file, return `codex-acp` in the same directory when it exists.
fn codex_acp_beside_resolved_codex(codex: &std::path::Path) -> Option<PathBuf> {
    let codex_file = if codex.is_absolute() {
        codex.to_path_buf()
    } else if codex.components().count() == 1 {
        #[cfg(unix)]
        {
            resolve_executable_on_path(codex.as_os_str())?
        }
        #[cfg(not(unix))]
        {
            return None;
        }
    } else {
        return None;
    };
    if !codex_file.is_file() {
        return None;
    }
    let parent = codex_file.parent()?;
    let acp = parent.join(tddy_core::backend::codex_acp::DEFAULT_CODEX_ACP_BINARY);
    if acp.is_file() {
        Some(acp)
    } else {
        None
    }
}

#[cfg(unix)]
fn resolve_executable_on_path(name: &std::ffi::OsStr) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            if let Ok(meta) = std::fs::metadata(&candidate) {
                if meta.permissions().mode() & 0o111 != 0 {
                    return Some(candidate);
                }
            }
        }
    }
    None
}
