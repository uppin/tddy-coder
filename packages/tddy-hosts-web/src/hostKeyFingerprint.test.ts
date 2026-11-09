/**
 * The fingerprint the browser derives for itself from a host's published key.
 *
 * The prompt carries a fingerprint *string* beside the key, over a channel the trust model calls a
 * trusted peer group and not an authenticated one. A string is not evidence: an active peer can
 * replay the genuine one beside its own key. What binds a sighting to the key an answer is actually
 * encrypted under is a digest of that key's bytes, computed here.
 *
 * The expectation is built with `node:crypto`, deliberately not through the module under test — a
 * round trip through our own formatting would prove only that we agree with ourselves, while what
 * has to hold is that we agree with `host_keypair.rs`'s `spki_fingerprint`.
 */

import { afterEach, describe, expect, it } from "bun:test";
import { createHash } from "node:crypto";
import { deriveHostKeyFingerprint } from "./hostKeyFingerprint";

const realCrypto = globalThis.crypto;

function setCrypto(value: unknown) {
  Object.defineProperty(globalThis, "crypto", { value, configurable: true, writable: true });
}

/** Stands in for a published SPKI DER — this function hashes bytes, and never imports a key. */
const A_PUBLISHED_KEY = new Uint8Array([0x30, 0x82, 0x01, 0x22, 0x30, 0x0d, 0x06, 0x09]);
const ANOTHER_PUBLISHED_KEY = new Uint8Array([0x30, 0x82, 0x01, 0x22, 0x30, 0x0d, 0x06, 0x0a]);

/** `SHA256:<base64 with no padding>`, computed the way `host_keypair.rs` computes it. */
function fingerprintTheDaemonWouldPublish(spkiDer: Uint8Array): string {
  const digest = createHash("sha256").update(spkiDer).digest("base64");
  return `SHA256:${digest.replace(/=+$/, "")}`;
}

describe("deriveHostKeyFingerprint", () => {
  afterEach(() => {
    setCrypto(realCrypto);
  });

  it("derives the fingerprint the daemon publishes for the same key", async () => {
    // Given a host's published key
    // When the browser derives a fingerprint for it
    const derived = await deriveHostKeyFingerprint(A_PUBLISHED_KEY);

    // Then it is the string `host_keypair.rs` would have published — same digest, same encoding,
    // so the two can be compared at all
    expect(derived).toBe(fingerprintTheDaemonWouldPublish(A_PUBLISHED_KEY));
  });

  it("derives a different fingerprint for a different key", async () => {
    // Given two keys that differ in one byte
    // When each is derived
    const first = await deriveHostKeyFingerprint(A_PUBLISHED_KEY);
    const second = await deriveHostKeyFingerprint(ANOTHER_PUBLISHED_KEY);

    // Then they do not collide — the whole point is that a substituted key cannot wear another
    // key's fingerprint
    expect(first).not.toBe(second);
  });

  it("refuses to derive on an origin that exposes no crypto.subtle, saying why", async () => {
    // Given the origin the daemon serves this bundle on: plain http, where `subtle` is withheld
    setCrypto({ getRandomValues: realCrypto.getRandomValues.bind(realCrypto) });

    // When a fingerprint is asked for
    const deriving = deriveHostKeyFingerprint(A_PUBLISHED_KEY);

    // Then it refuses out loud rather than returning a fingerprint of nothing
    await expect(deriving).rejects.toThrow(/secure context/i);
  });
});
