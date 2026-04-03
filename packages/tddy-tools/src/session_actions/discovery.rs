//! Enumerate `*.yaml` action definitions under `session_dir/actions/`.

use anyhow::{Context, Result};
use log::{debug, info};
use std::fs;
use std::path::{Path, PathBuf};

/// Return sorted paths to `*.yaml` / `*.yml` under `actions_dir` (no IO until implemented).
pub fn discover_action_yaml_paths(actions_dir: &Path) -> Result<Vec<PathBuf>> {
    info!(
        target: "tddy_tools::session_actions::discovery",
        "discover_action_yaml_paths dir={}",
        actions_dir.display()
    );
    let mut paths = Vec::new();
    if !actions_dir.exists() {
        debug!(
            target: "tddy_tools::session_actions::discovery",
            "actions_dir missing, returning empty"
        );
        return Ok(paths);
    }
    let rd =
        fs::read_dir(actions_dir).with_context(|| format!("read_dir {}", actions_dir.display()))?;
    for ent in rd {
        let ent = ent?;
        let p = ent.path();
        if !p.is_file() {
            continue;
        }
        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext == "yaml" || ext == "yml" {
            paths.push(p);
        }
    }
    paths.sort();
    debug!(
        target: "tddy_tools::session_actions::discovery",
        "discovered {} yaml files",
        paths.len()
    );
    Ok(paths)
}
