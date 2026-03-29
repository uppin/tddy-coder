//! Maps daemon `allowed_agents` config to display rows shared by ListAgents-style surfaces (PRD).

use crate::config::DaemonConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentAllowlistRow {
    pub id: String,
    pub display_label: String,
}

/// One row per `allowed_agents` entry, using the same label fallback rules as `ConnectionServiceImpl::list_agents`.
pub fn agent_allowlist_rows(config: &DaemonConfig) -> Vec<AgentAllowlistRow> {
    let entries = config.allowed_agents();
    log::debug!(
        "agent_allowlist_rows: building {} allowed_agents row(s)",
        entries.len()
    );
    entries
        .iter()
        .map(|a| {
            let display_label = a
                .label
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| a.id.clone());
            log::info!(
                "agent_allowlist_rows: id={} display_label={}",
                a.id,
                display_label
            );
            AgentAllowlistRow {
                id: a.id.clone(),
                display_label,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AllowedAgent, DaemonConfig};

    fn sample_config() -> DaemonConfig {
        DaemonConfig {
            allowed_agents: vec![
                AllowedAgent {
                    id: "zebra-backend".into(),
                    label: Some("Zebra".into()),
                },
                AllowedAgent {
                    id: "alpha-backend".into(),
                    label: None,
                },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn agent_allowlist_rows_match_list_agents_label_rules() {
        let rows = agent_allowlist_rows(&sample_config());
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "zebra-backend");
        assert_eq!(rows[0].display_label, "Zebra");
        assert_eq!(rows[1].id, "alpha-backend");
        assert_eq!(rows[1].display_label, "alpha-backend");
    }

    #[test]
    fn agent_allowlist_rows_blank_trimmed_label_falls_back_to_id() {
        let config = DaemonConfig {
            allowed_agents: vec![AllowedAgent {
                id: "only-id".into(),
                label: Some("   ".into()),
            }],
            ..Default::default()
        };
        let rows = agent_allowlist_rows(&config);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].display_label, "only-id");
    }
}
