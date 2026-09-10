//! `list-actions` / `invoke-action` CLI orchestration (Session Actions PRD).
//!
//! These functions are used as the LOCAL FALLBACK when `TDDY_SOCKET` is not set.
//! The relay path (when the socket is available) is handled in `cli.rs` directly.
//!
//! The listing and invocation themselves live in `tddy_core::session_actions`; what stays here is
//! the JSON written to stdout and the exit code an invocation failure classifies to.

use std::path::Path;

use log::{debug, info};

use tddy_core::session_actions::{
    classify_session_actions_exit_code, invoke_action_in_session_dir, list_actions_in_session_dir,
    DiscoveryQuery,
};

/// Local (non-relay) `list-actions` implementation.
pub fn run_list_actions(
    session_dir: &Path,
    path_prefix: Option<&str>,
    query_str: Option<&str>,
    limit: Option<usize>,
    offset: usize,
) -> anyhow::Result<()> {
    info!(
        target: "tddy_tools::session_actions_cli",
        "list-actions (local) session_dir={}",
        session_dir.display()
    );
    let query = DiscoveryQuery {
        path_prefix: path_prefix.map(str::to_owned),
        query: query_str.map(str::to_owned),
        limit,
        offset,
    };
    let out = list_actions_in_session_dir(session_dir, &query).map_err(anyhow::Error::from)?;
    println!("{}", serde_json::to_string(&out)?);
    Ok(())
}

/// Local (non-relay) `invoke-action` implementation.
pub fn run_invoke_action(
    session_dir: &Path,
    action_id: &str,
    data_json: &str,
) -> anyhow::Result<()> {
    debug!(
        target: "tddy_tools::session_actions_cli",
        "invoke-action (local) action_id={} session_dir={}",
        action_id,
        session_dir.display()
    );

    match invoke_action_in_session_dir(session_dir, action_id, data_json) {
        Ok(v) => {
            println!("{}", serde_json::to_string(&v)?);
            Ok(())
        }
        Err(e) => {
            let code = classify_session_actions_exit_code(&e);
            eprintln!("{e}");
            std::process::exit(code);
        }
    }
}
