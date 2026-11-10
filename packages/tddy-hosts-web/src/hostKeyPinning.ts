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
 * The value that is pinned is **derived from the key itself** (`hostKeyFingerprint`), never the
 * fingerprint string the prompt advertises beside it. Pinning the advertised string would make the
 * whole mechanism decorative: it is public and non-secret, so an active peer replays the genuine one
 * next to its own key, the check reports `unchanged`, the operator recognises the fingerprint they
 * verified out of band — and the answer is encrypted to the peer. Pinning the digest of the key that
 * will do the encrypting closes that, and a prompt whose two halves disagree is refused outright.
 *
 * Where the ingredients for a conclusion are missing — no key presented, or storage that refuses to
 * be read or written — the check degrades to `unverified` rather than to `pinned-now`. `pinned-now`
 * is a positive claim the dialog acts on ("first time seeing this host, key recorded"), and making
 * it when nothing was recorded would keep it false on every later sighting too, leaving an active
 * substitution indistinguishable from ordinary first use permanently. `unverified` says so out loud.
 */

import { deriveHostKeyFingerprint } from "./hostKeyFingerprint";

/** What a pin check concluded about a host's key. */
export type KeyPinVerdict =
  /** Never seen this host before; the key is now pinned. */
  | { kind: "pinned-now" }
  /** Same key as last time. */
  | { kind: "unchanged" }
  /** Different from the pinned key — the flow must stop and say so. */
  | { kind: "changed"; pinnedFingerprint: string }
  /**
   * The prompt's key and the fingerprint it advertised are not the same key.
   *
   * Not a rotation and not a first sighting: a host describing its own key gets it right, so the
   * two halves disagreeing means something rewrote one of them in flight. Nothing is pinned and
   * nothing may be sent.
   */
  | { kind: "mismatched"; advertisedFingerprint: string; derivedFingerprint: string }
  /** No continuity conclusion is available — no key was presented, or storage is unusable. */
  | { kind: "unverified" }
  /**
   * The key in hand has not been checked *yet* — its digest is still being derived.
   *
   * Never returned by {@link verifyHostKey}, which resolves to a conclusion: this is what a caller
   * holding a key whose check is outstanding says about it, and the reason the arm exists is that
   * the alternatives are all lies. `unverified` claims a check was attempted and reached nothing,
   * and does not block; `unchanged` claims continuity nobody has established. A key whose digest is
   * not back is a key nothing may be encrypted to, because "which key is this?" has no answer yet.
   */
  | { kind: "unchecked" }
  /**
   * The key could not be fingerprinted here at all — this origin exposes no `crypto.subtle`.
   *
   * Distinct from `unverified`, which is about storage: nothing on this page can hash a key or
   * encrypt an answer, so the flow is not merely uncheckable but unusable, and the reason says so.
   */
  | { kind: "underivable"; reason: string };

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
 * ⚠ `fingerprint` must be one **derived from a key** — {@link verifyHostKey} is the entry point a
 * caller holding a `HostPromptEvent` wants. Passing the fingerprint a prompt advertised makes the
 * pin decorative, since that string is public and a substituting peer can send it verbatim.
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

// ---------------------------------------------------------------------------
// Checking a prompt's key against the pin
// ---------------------------------------------------------------------------

/** What a prompt's key turned out to be, and what this browser had on record for it. */
export interface HostKeyCheck {
  /** What the sighting means for the flow — what the dialog blocks on, and what it says. */
  verdict: KeyPinVerdict;
  /**
   * The fingerprint derived from the key that arrived, or `null` where none could be derived.
   *
   * This — not the advertised string — is what the dialog displays and what
   * {@link acceptChangedHostKey} records, so the value an operator verifies out of band is the
   * value bound to the key their passphrase is encrypted under.
   */
  fingerprint: string | null;
}

/**
 * Check the key a prompt carried against what is pinned for `hostId`, pinning it on a first sighting.
 *
 * The advertised fingerprint is used for one thing only: catching a prompt whose two halves disagree.
 * It is never pinned, never displayed and never compared against the pin, because a peer that
 * substitutes the key can advertise whatever string it likes beside it.
 *
 * Resolves for every outcome, including the ones it could not reach a conclusion in — a check that
 * threw would take down the row that renders the dialog.
 */
export async function verifyHostKey(
  hostId: string,
  spkiDer: Uint8Array,
  advertisedFingerprint: string,
): Promise<HostKeyCheck> {
  if (spkiDer.length === 0) {
    // No key presented, so there is nothing to encrypt under and nothing to hash. Pinning here would
    // spend the first-use trust slot on nothing.
    return { verdict: { kind: "unverified" }, fingerprint: null };
  }

  let derived: string;
  try {
    derived = await deriveHostKeyFingerprint(spkiDer);
  } catch (error) {
    return {
      verdict: {
        kind: "underivable",
        reason: error instanceof Error ? error.message : String(error),
      },
      fingerprint: null,
    };
  }

  if (derived !== advertisedFingerprint) {
    // Reported rather than quietly preferred: a genuine host publishes a fingerprint of its own key,
    // so this is a frame that was rewritten between the host and here — the substitution the pin
    // exists to catch, caught before the first sighting rather than after it.
    return {
      verdict: { kind: "mismatched", advertisedFingerprint, derivedFingerprint: derived },
      fingerprint: derived,
    };
  }

  return { verdict: checkHostKey(hostId, derived), fingerprint: derived };
}
