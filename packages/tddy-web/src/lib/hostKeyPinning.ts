/**
 * Key continuity for host prompt keys — trust on first use, warn loudly on change.
 *
 * Encrypting an answer under a host's public key defeats a passive relay, but the browser learns
 * that key over the same channel the trust model calls "a trusted peer group, **not a
 * cryptographically authenticated one**". A hostile participant advertising itself as
 * `daemon-<instance_id>` could publish its own key and read what we send.
 *
 * Pinning bounds that: the first key seen for a host is remembered, and a *changed* key blocks the
 * flow until an operator accepts it explicitly. It is SSH's own model, and the same one an operator
 * already recognises from `REMOTE HOST IDENTIFICATION HAS CHANGED`.
 *
 * ⚠ This is the stack's most arguable design decision. It does not make an active substitution
 * impossible — only visible, and only after the first sighting. The alternative considered was to
 * accept passive-only protection and disclose it in the dialog.
 */

/** What a pin check concluded about a host's key. */
export type KeyPinVerdict =
  /** Never seen this host before; the key is now pinned. */
  | { kind: "pinned-now" }
  /** Same key as last time. */
  | { kind: "unchanged" }
  /** Different from the pinned key — the flow must stop and say so. */
  | { kind: "changed"; pinnedFingerprint: string };

/** One `localStorage` entry per host, so a pin can be dropped without touching the others. */
const PIN_KEY_PREFIX = "tddy.hostKeyPin.";

function pinKey(hostId: string): string {
  return `${PIN_KEY_PREFIX}${hostId}`;
}

/**
 * The pinned fingerprint for `hostId`, or `null` when there is none *or* storage is unusable.
 *
 * Private-mode and storage-blocked browsers throw on access rather than returning `null`; letting
 * that escape would take the whole hosts screen down over a hardening feature.
 */
function readPin(hostId: string): string | null {
  try {
    return window.localStorage.getItem(pinKey(hostId));
  } catch {
    return null;
  }
}

/** Record `fingerprint` as the pin for `hostId`. A storage that refuses the write is not fatal. */
function writePin(hostId: string, fingerprint: string): void {
  try {
    window.localStorage.setItem(pinKey(hostId), fingerprint);
  } catch {
    // Nothing is pinned, so the next sighting is a first sighting again — see `checkHostKey`.
  }
}

/**
 * Check `fingerprint` against what is pinned for `hostId`, pinning it when nothing is.
 *
 * Per-browser by design: a pin is a record of what *this* operator saw, and syncing it through the
 * daemon would route the trust anchor back through the channel it exists to distrust.
 *
 * ⚠ Where storage is unavailable every sighting reads as a first sighting (`pinned-now`), which is
 * trust-on-every-use: no worse than the no-pinning alternative, and never a false `unchanged`.
 */
export function checkHostKey(hostId: string, fingerprint: string): KeyPinVerdict {
  const pinnedFingerprint = readPin(hostId);
  if (pinnedFingerprint === null) {
    writePin(hostId, fingerprint);
    return { kind: "pinned-now" };
  }
  if (pinnedFingerprint === fingerprint) {
    return { kind: "unchanged" };
  }
  return { kind: "changed", pinnedFingerprint };
}

/**
 * Accept `fingerprint` as the new pin for `hostId`, after an operator has said the change is theirs.
 *
 * The new key is pinned outright rather than the old pin merely dropped: dropping it would silently
 * accept whichever key turned up next, which is the sighting the operator did *not* look at.
 */
export function acceptChangedHostKey(hostId: string, fingerprint: string): void {
  writePin(hostId, fingerprint);
}
