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

/**
 * Check `fingerprint` against what is pinned for `hostId`, pinning it when nothing is.
 *
 * Per-browser by design: a pin is a record of what *this* operator saw, and syncing it through the
 * daemon would route the trust anchor back through the channel it exists to distrust.
 */
export function checkHostKey(hostId: string, fingerprint: string): KeyPinVerdict {
  // TODO(agent-add-key): implement
  void hostId;
  void fingerprint;
  throw new Error("agent-add-key: checkHostKey not implemented");
}

/** Forget the pin for `hostId`, so the next sighting pins afresh. Used when an operator accepts a changed key. */
export function acceptChangedHostKey(hostId: string, fingerprint: string): void {
  // TODO(agent-add-key): implement
  void hostId;
  void fingerprint;
  throw new Error("agent-add-key: acceptChangedHostKey not implemented");
}
