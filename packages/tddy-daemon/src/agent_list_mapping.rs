//! Maps daemon `allowed_agents` config to display rows shared by ListAgents-style surfaces (PRD).

use tddy_service::proto::models::AssistantEntry;

use crate::config::DaemonConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentAllowlistRow {
    pub id: String,
    pub display_label: String,
}

/// The agents this daemon can start a session as: one row per `allowed_agents` config entry,
/// followed by one per assistant in the daemon's model registry.
///
/// Both sources feed `--agent <id>` — a config entry names a coding backend, an assistant names a
/// `SpecializedAgentDef` projected from the registry — so they belong in one list. Config comes
/// first because those are the daemon operator's own, deliberately-listed backends.
///
/// Labels use the same fallback rule throughout: a blank label falls back to the id.
pub fn agent_allowlist_rows(
    config: &DaemonConfig,
    assistants: &[AssistantEntry],
) -> Vec<AgentAllowlistRow> {
    let entries = config.allowed_agents();
    log::debug!(
        "agent_allowlist_rows: building {} allowed_agents row(s) and {} assistant row(s)",
        entries.len(),
        assistants.len()
    );
    let configured = entries.iter().map(|a| {
        let display_label = labelled(a.label.as_deref(), &a.id);
        log::info!(
            "agent_allowlist_rows: id={} display_label={}",
            a.id,
            display_label
        );
        AgentAllowlistRow {
            id: a.id.clone(),
            display_label,
        }
    });
    let registry = assistants.iter().map(|assistant| AgentAllowlistRow {
        id: assistant.name.clone(),
        display_label: labelled(Some(assistant.label.as_str()), &assistant.name),
    });
    configured.chain(registry).collect()
}

/// `label` when it carries something after trimming, else the `id` it belongs to.
fn labelled(label: Option<&str>, id: &str) -> String {
    label
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| id.to_string())
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
        let rows = agent_allowlist_rows(&sample_config(), &[]);
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
        let rows = agent_allowlist_rows(&config, &[]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].display_label, "only-id");
    }
}
