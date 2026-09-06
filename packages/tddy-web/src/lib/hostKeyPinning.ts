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
 *
 * Where the ingredients for a conclusion are missing — no key presented, or storage that refuses to
 * be read or written — the check degrades to `unverified` rather than to `pinned-now`. `pinned-now`
 * is a positive claim the dialog acts on ("first time seeing this host, key recorded"), and making
 * it when nothing was recorded would keep it false on every later sighting too, leaving an active
 * substitution indistinguishable from ordinary first use permanently. `unverified` says so out loud.
 */

/** What a pin check concluded about a host's key. */
export type KeyPinVerdict =
  /** Never seen this host before; the key is now pinned. */
  | { kind: "pinned-now" }
  /** Same key as last time. */
  | { kind: "unchanged" }
  /** Different from the pinned key — the flow must stop and say so. */
  | { kind: "changed"; pinnedFingerprint: string }
  /** No continuity conclusion is available — no key was presented, or storage is unusable. */
  | { kind: "unverified" };

/** One `localStorage` entry per host, so a pin can be dropped without touching the others. */
const PIN_KEY_PREFIX = "tddy.hostKeyPin.";

function pinKey(hostId: string): string {
  return `${PIN_KEY_PREFIX}${hostId}`;
}

/** What storage could tell us about a host's pin — including that it could tell us nothing. */
type PinRead =
  /** A fingerprint is on record for this host. */
  | { kind: "pinned"; fingerprint: string }
  /** Storage answered, and holds no pin for this host. */
  | { kind: "absent" }
  /** Storage refused to answer, so "no pin" and "a pin we cannot see" are indistinguishable. */
  | { kind: "unreadable" };

/**
 * What is pinned for `hostId`, keeping "storage says nothing is pinned" apart from "storage will
 * not say".
 *
 * Private-mode and storage-blocked browsers throw on access rather than returning `null`; letting
 * that escape would take the whole hosts screen down over a hardening feature. Collapsing it into
 * `absent` instead would turn a refused read into a fabricated first sighting.
 */
function readPin(hostId: string): PinRead {
  let stored: string | null;
  try {
    stored = window.localStorage.getItem(pinKey(hostId));
  } catch {
    return { kind: "unreadable" };
  }
  return stored === null ? { kind: "absent" } : { kind: "pinned", fingerprint: stored };
}

/**
 * Record `fingerprint` as the pin for `hostId`, reporting whether the write actually landed.
 *
 * A storage that refuses the write is not fatal, but it is not a pin either, so the caller has to
 * know rather than assume.
 */
function writePin(hostId: string, fingerprint: string): boolean {
  try {
    window.localStorage.setItem(pinKey(hostId), fingerprint);
    return true;
  } catch {
    return false;
  }
}

/**
 * Check `fingerprint` against what is pinned for `hostId`, pinning it when nothing is.
 *
 * Per-browser by design: a pin is a record of what *this* operator saw, and syncing it through the
 * daemon would route the trust anchor back through the channel it exists to distrust.
 *
 * An empty `fingerprint` is not key material: pinning it would spend the single first-use trust slot
 * on nothing, after which the host's genuine key would read as `changed` from `""`.
 */
export function checkHostKey(hostId: string, fingerprint: string): KeyPinVerdict {
  if (fingerprint === "") {
    return { kind: "unverified" };
  }
  const pin = readPin(hostId);
  if (pin.kind === "unreadable") {
    return { kind: "unverified" };
  }
  if (pin.kind === "absent") {
    return writePin(hostId, fingerprint) ? { kind: "pinned-now" } : { kind: "unverified" };
  }
  if (pin.fingerprint === fingerprint) {
    return { kind: "unchanged" };
  }
  return { kind: "changed", pinnedFingerprint: pin.fingerprint };
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
