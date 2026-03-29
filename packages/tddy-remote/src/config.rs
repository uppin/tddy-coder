//! Client config (authorities, Connect base URLs).

use std::path::Path;

use serde::Deserialize;

/// Errors loading or parsing `tddy-remote` YAML config.
#[derive(Debug, thiserror::Error)]
pub enum RemoteConfigError {
    #[error("remote config: {0}")]
    Message(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("yaml: {0}")]
    Yaml(#[from] serde_yaml::Error),
}

#[derive(Debug, Deserialize)]
struct RemoteYaml {
    authorities: Vec<AuthorityYaml>,
    #[serde(default)]
    default_authority: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AuthorityYaml {
    id: String,
    connect_base_url: String,
}

fn connect_url_for_authority_id(authorities: &[AuthorityYaml], id: &str) -> Option<String> {
    authorities
        .iter()
        .find(|a| a.id == id)
        .map(|a| a.connect_base_url.trim_end_matches('/').to_string())
}

/// Load configured authority ids from a YAML file (authorities[].id).
pub fn load_authority_ids_from_path(path: &Path) -> Result<Vec<String>, RemoteConfigError> {
    log::debug!("load_authority_ids_from_path path={}", path.display());
    let raw = std::fs::read_to_string(path)?;
    load_authority_ids_from_yaml(&raw)
}

/// Parse authority ids from YAML text (unit/integration tests).
pub fn load_authority_ids_from_yaml(yaml: &str) -> Result<Vec<String>, RemoteConfigError> {
    log::debug!("load_authority_ids_from_yaml bytes={}", yaml.len());
    let cfg: RemoteYaml = serde_yaml::from_str(yaml)?;
    Ok(cfg.authorities.into_iter().map(|a| a.id).collect())
}

/// Resolved Connect base URL for an authority id or host string.
pub(crate) fn resolve_connect_base(
    path: Option<&Path>,
    host: &str,
) -> Result<String, RemoteConfigError> {
    let Some(p) = path else {
        return Err(RemoteConfigError::Message(
            "--config is required for this command".into(),
        ));
    };
    let raw = std::fs::read_to_string(p)?;
    let cfg: RemoteYaml = serde_yaml::from_str(&raw)?;

    let primary_id = if host.trim().is_empty() {
        cfg.default_authority.as_deref().ok_or_else(|| {
            RemoteConfigError::Message(
                "authority host is empty; set default_authority in config or pass HOST".into(),
            )
        })?
    } else {
        host
    };

    if let Some(url) = connect_url_for_authority_id(&cfg.authorities, primary_id) {
        log::info!("resolved authority host={primary_id} url={url}");
        return Ok(url);
    }

    if !host.trim().is_empty() {
        if let Some(def) = cfg.default_authority.as_deref() {
            if let Some(url) = connect_url_for_authority_id(&cfg.authorities, def) {
                log::info!("resolved authority via default_authority fallback id={def} url={url}");
                return Ok(url);
            }
        }
    }

    Err(RemoteConfigError::Message(format!(
        "unknown authority or host {host:?} (check authorities[].id in config)"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_authority_used_when_host_unknown() {
        let yaml = r#"
default_authority: "unit-fixture-alpha"
authorities:
  - id: "unit-fixture-alpha"
    connect_base_url: "http://127.0.0.1:9000"
"#;
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("remote.yaml");
        std::fs::write(&path, yaml).expect("write");
        let url = resolve_connect_base(Some(&path), "typo-host").expect("resolve");
        assert_eq!(url, "http://127.0.0.1:9000");
    }

    #[test]
    fn empty_host_uses_default_authority_id() {
        let yaml = r#"
default_authority: "unit-fixture-alpha"
authorities:
  - id: "unit-fixture-alpha"
    connect_base_url: "http://127.0.0.1:9001"
"#;
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("remote.yaml");
        std::fs::write(&path, yaml).expect("write");
        let url = resolve_connect_base(Some(&path), "").expect("resolve");
        assert_eq!(url, "http://127.0.0.1:9001");
    }
}
