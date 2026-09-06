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
  /** The host's base clone location (`repos_base_path`), relative to each OS user's home, as
   *  advertised in the common room. Optional: older daemons don't advertise it. */
  reposBasePath?: string;
  /** The largest single session attachment this host will serve (`max_attachment_bytes`), as
   *  advertised in the common room. Optional: older daemons don't advertise it. */
  maxAttachmentBytes?: number;
}

/**
 * How a daemon self-labels its own advertisement — `"{id} (this daemon)"`.
 *
 * One constant because three places depend on the same spelling and none of them can see the
 * others: {@link parseDaemonAdvertisement} requires it, `shell/DaemonSelector` strips it from every
 * host that is not the serving one, and `rpc/hostDirectory/servingSource` writes it for a serving
 * host named by `/api/config`, which has no advertisement to copy a label from. Change it in one
 * place and every host would render with a stray suffix.
 */
export const SELF_LABEL_SUFFIX = " (this daemon)";

/**
 * Parse a `tddy-daemon` common-room advertisement (`livekit_peer_discovery`):
 * `{"instance_id":"…","label":"… (this daemon)"}`. Returns `null` when the metadata is not a
 * well-formed daemon advertisement.
 */
export function parseDaemonAdvertisement(metadata: string): DaemonHost | null {
  const t = metadata.trim();
  if (!t.startsWith("{")) return null;
  try {
    const o = JSON.parse(t) as {
      instance_id?: unknown;
      label?: unknown;
      repos_base_path?: unknown;
      max_attachment_bytes?: unknown;
    };
    if (typeof o.instance_id !== "string" || !o.instance_id.trim()) return null;
    // `includes`, not `endsWith`: a daemon may append its own trailing detail after the suffix, and
    // the leading space is not required of an advertisement whose label is only the suffix itself.
    if (typeof o.label !== "string" || !o.label.includes(SELF_LABEL_SUFFIX.trim())) return null;
    const host: DaemonHost = { instanceId: o.instance_id.trim(), label: o.label.trim() };
    if (typeof o.repos_base_path === "string" && o.repos_base_path.trim()) {
      host.reposBasePath = o.repos_base_path.trim();
    }
    // A cap of 0 (or worse) would make every attachment unpickable, so it reads as "unadvertised".
    const cap = o.max_attachment_bytes;
    if (typeof cap === "number" && Number.isFinite(cap) && cap > 0) {
      host.maxAttachmentBytes = cap;
    }
    return host;
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
    const host: DaemonHost = { instanceId, label: adv?.label || instanceId };
    if (adv?.reposBasePath) host.reposBasePath = adv.reposBasePath;
    // The Start-Session form enforces this cap at pick time, and this list is its only source of
    // hosts — a cap dropped here is a cap the form can never enforce.
    if (adv?.maxAttachmentBytes) host.maxAttachmentBytes = adv.maxAttachmentBytes;
    hosts.push(host);
  }
  return hosts;
}

/**
 * A daemon joins the common room as two participants: its discovery identity (the bare
 * instance id, what {@link daemonHostsFromParticipants} lists) and its RPC-server identity,
 * `daemon-{instanceId}` — see `tddy-daemon`'s `main.rs` (`rpc_identity = format!("daemon-{local_id}")`).
 * Daemon-level RPC must address the latter.
 */
export function daemonRpcIdentity(instanceId: string): string {
  return `daemon-${instanceId}`;
}
