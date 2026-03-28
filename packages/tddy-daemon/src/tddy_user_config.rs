//! Per–OS-user settings read from `~/.tddy/config.yaml` (home of the user `tddy-coder` runs as).

use std::path::Path;

/// YAML schema for `{home}/.tddy/config.yaml`.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TddyUserHomeConfig {
    /// Colon-separated directories prepended to `PATH` for spawned `tddy-coder` (e.g. Cursor `agent`).
    #[serde(default)]
    pub spawn_path_extra: Option<String>,
}

/// Load `~/.tddy/config.yaml` under `home`. Missing file returns `None`; parse errors are logged.
pub fn load_tddy_user_config(home: &Path) -> Option<TddyUserHomeConfig> {
    let path = home.join(".tddy").join("config.yaml");
    if !path.is_file() {
        return None;
    }
    let contents = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("tddy user config: read {}: {}", path.display(), e);
            return None;
        }
    };
    match serde_yaml::from_str::<TddyUserHomeConfig>(&contents) {
        Ok(c) => Some(c),
        Err(e) => {
            log::warn!("tddy user config: parse {}: {}", path.display(), e);
            None
        }
    }
}

/// `spawn_path_extra` from the target user's `~/.tddy/config.yaml`, if set and non-empty.
pub fn spawn_path_extra_for_home(home: &Path) -> Option<String> {
    load_tddy_user_config(home).and_then(|c| {
        c.spawn_path_extra
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    })
}

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
