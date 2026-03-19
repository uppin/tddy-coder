//! tddy-demo — Same app as tddy-coder, with StubBackend.
//!
//! Identical TUI, CLI, and workflow. Only the backend differs (StubBackend).
//! Uses tddy-coder lib with DemoArgs (agent: stub only).
//!
//! Demo-specific: cleans up stale worktrees from previous runs before starting.

use std::env;
use std::path::Path;

use clap::Parser;
use tddy_coder::{load_config, merge_config_into_args, run_main, Args, DemoArgs};
use tddy_core::{remove_worktree, worktree_dir};

/// Clean up stale worktrees in `.worktrees/` that were left by previous demo runs.
/// Only removes worktrees whose directory still exists on disk.
fn cleanup_stale_worktrees(repo_root: &Path) {
    let wt_dir = worktree_dir(repo_root);
    if !wt_dir.exists() {
        return;
    }
    let entries = match std::fs::read_dir(&wt_dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Err(e) = remove_worktree(repo_root, &path) {
                eprintln!(
                    "Warning: failed to remove stale worktree {}: {}",
                    path.display(),
                    e
                );
            }
            // Also remove the branch (git worktree remove doesn't delete it)
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                let branch = format!("feature/{}", name);
                let _ = std::process::Command::new("git")
                    .args(["branch", "-D", &branch])
                    .current_dir(repo_root)
                    .output();
            }
        }
    }
}

fn main() {
    let mut cli_args: Vec<String> = env::args().collect();
    let has_agent = cli_args
        .iter()
        .skip(1)
        .position(|a| a == "--agent" || a == "-a");
    if has_agent.is_none() {
        cli_args.insert(1, "stub".to_string());
        cli_args.insert(1, "--agent".to_string());
    }
    let demo_args = DemoArgs::parse_from(cli_args);
    let config_path = demo_args.config.clone();
    let mut args: Args = demo_args.into();

    if let Some(ref path) = config_path {
        match load_config(path) {
            Ok(config) => merge_config_into_args(&mut args, config),
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
    }

    // Demo-specific: clean up stale worktrees before starting
    let repo_root = std::env::current_dir().unwrap_or_default();
    cleanup_stale_worktrees(&repo_root);

    run_main(args);
}
