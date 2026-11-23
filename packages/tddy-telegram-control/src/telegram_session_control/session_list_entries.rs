use crate::telegram_session_control::*;

pub(super) fn resolve_child_grpc_port(
    map: &HashMap<String, u16>,
    session_key: &str,
) -> anyhow::Result<(String, u16)> {
    if let Some(&p) = map.get(session_key) {
        return Ok((session_key.to_string(), p));
    }
    let mut hits: Vec<(String, u16)> = map
        .iter()
        .filter(|(sid, _)| sid.as_str().starts_with(session_key))
        .map(|(s, &p)| (s.clone(), p))
        .collect();
    match hits.len() {
        0 => anyhow::bail!(
            "No active workflow found for session {:?}. \
Start a workflow from Telegram (recipe → project → agent), or use the full session id.",
            session_key
        ),
        1 => Ok(hits.pop().expect("len 1")),
        _ => anyhow::bail!(
            "Ambiguous session prefix {:?}; use more hex characters or the full session id.",
            session_key
        ),
    }
}

/// Format a single session entry as a Telegram-friendly text line.
pub fn format_session_list_entry(entry: &TelegramSessionEntry) -> String {
    format!(
        "{} · {} · {} · {}",
        entry.label, entry.status, entry.workflow_state, entry.elapsed_display
    )
}

pub(super) fn telegram_label_for_session_id(session_id: &str) -> String {
    session_telegram_label(session_id).unwrap_or_else(|| session_id.to_string())
}

pub(super) fn session_list_status_or_placeholders(
    agents: &tddy_coding_agent_specs::CodingAgentRegistry,
    session_dir: &Path,
) -> SessionListStatusDisplay {
    match tddy_session_lifecycle::session_list_enrichment::session_list_status_from_session_dir(
        Some(agents),
        session_dir,
    ) {
        Ok(d) => d,
        Err(_) => SessionListStatusDisplay {
            workflow_goal: "—".to_string(),
            workflow_state: "—".to_string(),
            elapsed_display: "—".to_string(),
            agent: "—".to_string(),
            model: "—".to_string(),
            activity_status: String::new(),
            orchestrator_session_id: String::new(),
            recipe: String::new(),
            work_queue_json: String::new(),
            queue_id: String::new(),
            branch: String::new(),
        },
    }
}
