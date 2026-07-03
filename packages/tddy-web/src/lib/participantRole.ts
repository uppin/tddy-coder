/**
 * LiveKit participant role derivation — the single source of truth for classifying a common-room
 * participant as a browser, coder/session, or `tddy-daemon` from its identity + metadata.
 *
 * Used by the presence table (`useRoomParticipants`) and by daemon/host selection (Projects screen).
 * The daemon side mirrors this logic in `tddy-daemon`'s `eligible_daemon_from_participant_fields`.
 */

export type ParticipantRole = "browser" | "coder" | "daemon" | "unknown";

/** A daemon host that can own projects and receive sessions, derived from a common-room participant. */
export interface DaemonHost {
  instanceId: string;
  label: string;
}

/**
 * Parse a `tddy-daemon` common-room advertisement (`livekit_peer_discovery`):
 * `{"instance_id":"…","label":"… (this daemon)"}`. Returns `null` when the metadata is not a
 * well-formed daemon advertisement.
 */
export function parseDaemonAdvertisement(metadata: string): DaemonHost | null {
  const t = metadata.trim();
  if (!t.startsWith("{")) return null;
  try {
    const o = JSON.parse(t) as { instance_id?: unknown; label?: unknown };
    if (typeof o.instance_id !== "string" || !o.instance_id.trim()) return null;
    if (typeof o.label !== "string" || !o.label.includes("(this daemon)")) return null;
    return { instanceId: o.instance_id.trim(), label: o.label.trim() };
  } catch {
    return null;
  }
}

/** Whether `metadata` is a well-formed `tddy-daemon` advertisement. */
export function metadataLooksLikeDaemonAdvertisement(metadata: string): boolean {
  return parseDaemonAdvertisement(metadata) !== null;
}

/**
 * Infer UI role from LiveKit identity and metadata.
 * - **browser**: dashboard presence (`web-…`, `browser-…`).
 * - **coder**: terminal/session tool side (`server`, `server…`, `daemon-{uuid}-…`).
 * - **daemon**: embedded/CLI `tddy-daemon` in common room (metadata advertisement).
 */
export function inferParticipantRole(identity: string, metadata: string): ParticipantRole {
  if (identity.startsWith("web-") || identity.startsWith("browser-")) return "browser";
  if (
    identity === "server" ||
    identity.startsWith("server") ||
    identity.startsWith("daemon-")
  ) {
    return "coder";
  }
  if (metadataLooksLikeDaemonAdvertisement(metadata)) {
    return "daemon";
  }
  return "unknown";
}

/**
 * Derive the eligible daemon hosts from a set of common-room participants: keep only those whose
 * role is `daemon`, using the advertisement's `instance_id`/`label` (falling back to the LiveKit
 * identity for the id). Deduplicated by `instanceId`, preserving first-seen order.
 */
export function daemonHostsFromParticipants(
  participants: { identity: string; metadata: string }[],
): DaemonHost[] {
  const hosts: DaemonHost[] = [];
  const seen = new Set<string>();
  for (const p of participants) {
    if (inferParticipantRole(p.identity, p.metadata) !== "daemon") continue;
    const adv = parseDaemonAdvertisement(p.metadata);
    const instanceId = (adv?.instanceId ?? p.identity).trim();
    if (!instanceId || seen.has(instanceId)) continue;
    seen.add(instanceId);
    hosts.push({ instanceId, label: adv?.label || instanceId });
  }
  return hosts;
}
