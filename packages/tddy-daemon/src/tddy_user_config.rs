//! Per–OS-user settings read from `~/.tddy/config.yaml` (home of the user `tddy-coder` runs as).
//!
//! The reader itself lives in [`tddy_daemon_kernel::user_paths`], because
//! `tddy-worktree-service`'s `remote_git_service` builds a child environment with it and cannot
//! reach into this crate. This re-export keeps `crate::tddy_user_config::…` resolving, and the
//! tests below stay here because they are this module's contract with its callers.

pub use tddy_daemon_kernel::user_paths::{
    load_tddy_user_config, spawn_path_extra_for_home, TddyUserHomeConfig,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_path_extra_reads_yaml_under_dot_tddy() {
        let tmp = tempfile::tempdir().unwrap();
        let tddy = tmp.path().join(".tddy");
        std::fs::create_dir_all(&tddy).unwrap();
        std::fs::write(
            tddy.join("config.yaml"),
            "spawn_path_extra: \"/opt/cursor/bin:/extra\"\n",
        )
        .unwrap();
        assert_eq!(
            spawn_path_extra_for_home(tmp.path()).as_deref(),
            Some("/opt/cursor/bin:/extra")
        );
    }

    #[test]
    fn missing_file_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(spawn_path_extra_for_home(tmp.path()), None);
    }
}
