use crate::host_registry::HostSighting;
use crate::host_registry::KnownHost;
use crate::host_registry::LAST_SEEN_REFRESH_FLOOR_MS;
use crate::host_registry::MAX_INSTANCE_ID_BYTES;
use crate::host_registry::MAX_KNOWN_HOSTS;
use crate::host_registry::MAX_LABEL_BYTES;
use crate::host_registry::MAX_REPOS_BASE_PATH_BYTES;
use crate::multi_host::DaemonInstanceId;

/// Apply one sighting to `hosts`, reporting whether it changed anything worth a write.
///
/// A `last_seen` that merely advanced is deliberately **not** a change — it advances on every tick,
/// so counting it would make every tick a write, which is the whole thing this store avoids. One
/// that has fallen [`LAST_SEEN_REFRESH_FLOOR_MS`] behind *is*, because a stamp allowed to age
/// without limit stops meaning anything.
pub(super) fn apply_sighting(
    hosts: &mut Vec<KnownHost>,
    sighting: &HostSighting,
    now_unix_ms: i64,
) -> bool {
    let instance_id = match validated_instance_id(&sighting.instance_id.0) {
        Ok(id) => id,
        Err(e) => {
            // One malformed peer must not cost the rest of the snapshot its recording.
            log::warn!("host registry: refusing to record a host: {e}");
            return false;
        }
    };
    let label = bounded_label(&sighting.label);
    let repos_base_path = bounded_repos_base_path(sighting.repos_base_path.as_deref());
    let Some(known) = hosts
        .iter_mut()
        .find(|host| host.instance_id == instance_id)
    else {
        if hosts.len() >= MAX_KNOWN_HOSTS {
            log::warn!(
                "host registry: at the {MAX_KNOWN_HOSTS}-host ceiling; not recording new host {instance_id}"
            );
            return false;
        }
        hosts.push(KnownHost {
            label: if label.is_empty() {
                instance_id.clone()
            } else {
                label
            },
            instance_id,
            first_seen_unix_ms: now_unix_ms,
            last_seen_unix_ms: now_unix_ms,
            repos_base_path: repos_base_path.unwrap_or_default(),
            max_attachment_bytes: sighting.max_attachment_bytes.unwrap_or_default(),
        });
        return true;
    };

    let mut changed =
        now_unix_ms.saturating_sub(known.last_seen_unix_ms) >= LAST_SEEN_REFRESH_FLOOR_MS;
    if !label.is_empty() && known.label != label {
        known.label = label;
        changed = true;
    }
    if let Some(repos_base_path) = repos_base_path {
        if known.repos_base_path != repos_base_path {
            known.repos_base_path = repos_base_path;
            changed = true;
        }
    }
    if let Some(max_attachment_bytes) = sighting.max_attachment_bytes {
        if known.max_attachment_bytes != max_attachment_bytes {
            known.max_attachment_bytes = max_attachment_bytes;
            changed = true;
        }
    }
    changed
}

/// Stamp a departure, reporting whether there was an entry to stamp.
pub(super) fn apply_departure(
    hosts: &mut [KnownHost],
    instance_id: &DaemonInstanceId,
    now_unix_ms: i64,
) -> bool {
    let Some(known) = hosts
        .iter_mut()
        .find(|host| host.instance_id == instance_id.0.trim())
    else {
        // A departure we never saw arrive. Creating an entry for it would record a sighting that
        // never happened, so there is nothing to persist and nothing to report.
        return false;
    };
    if known.last_seen_unix_ms == now_unix_ms {
        return false;
    }
    known.last_seen_unix_ms = now_unix_ms;
    true
}

/// A host id fit to be written down, or why it is not.
fn validated_instance_id(raw: &str) -> Result<String, String> {
    let id = raw.trim();
    if id.is_empty() {
        return Err("the host id is empty".to_string());
    }
    if id.len() > MAX_INSTANCE_ID_BYTES {
        return Err(format!(
            "the host id is {} bytes, over the {MAX_INSTANCE_ID_BYTES}-byte limit",
            id.len()
        ));
    }
    if let Some(bad) = id.chars().find(|c| !is_instance_id_char(*c)) {
        return Err(format!("the host id {id:?} contains {bad:?}"));
    }
    Ok(id.to_string())
}

/// The characters a host id may be spelled with: hostnames, configured ids and the startup-suffix
/// digits, and nothing that could turn a registry row into markup or a path.
fn is_instance_id_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':' | '@' | '+')
}

/// A label short enough to store. Display text, so cutting it down is honest.
fn bounded_label(raw: &str) -> String {
    truncated_to(raw.trim(), MAX_LABEL_BYTES).to_string()
}

/// An observed repos base path short enough to store, or nothing observed.
fn bounded_repos_base_path(raw: Option<&str>) -> Option<String> {
    let path = raw?.trim();
    if path.is_empty() {
        return None;
    }
    if path.len() > MAX_REPOS_BASE_PATH_BYTES {
        // Half a path points somewhere else entirely, so this column is better left unset than
        // shown wrong.
        log::warn!(
            "host registry: ignoring a repos base path of {} bytes (over the {MAX_REPOS_BASE_PATH_BYTES}-byte limit)",
            path.len()
        );
        return None;
    }
    Some(path.to_string())
}

/// `text` cut to at most `max_bytes`, never mid-character.
fn truncated_to(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}
