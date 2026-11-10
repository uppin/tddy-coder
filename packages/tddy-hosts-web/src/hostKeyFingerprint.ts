/**
 * The fingerprint of a host's published key, derived in the browser from the key itself.
 *
 * A `HostPromptEvent` carries two things about the host's identity: the SPKI DER an answer is
 * encrypted under, and a fingerprint *string*. Only the first one encrypts anything. Both ride the
 * same channel, which `docs/ft/web/projects-screen-multi-host.md` calls "a trusted peer group,
 * **not** a cryptographically authenticated one" — so the string is an assertion by whoever sent
 * the frame, and a non-secret one an active peer can replay verbatim beside its own key.
 *
 * Deriving it here makes the fingerprint an observation instead: it is a digest of exactly the bytes
 * that will encrypt the passphrase, so what an operator verifies out of band and what the pin
 * records are the same fact about the same key.
 *
 * The encoding matches `packages/tddy-daemon/src/host_keypair.rs`'s `spki_fingerprint` — SHA-256 over
 * the published SPKI DER, base64 without padding, prefixed `SHA256:` — because a value that cannot
 * be compared with the daemon's is no better than not deriving one.
 */

import { requireSubtleCrypto } from "./subtleCrypto";

/** Base64 as the daemon writes it: standard alphabet, padding stripped, the way `ssh-add -l` reads. */
function base64NoPad(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) {
    binary += String.fromCharCode(byte);
  }
  return btoa(binary).replace(/=+$/, "");
}

/**
 * Derive `SHA256:<base64>` for a host's published SPKI DER public key.
 *
 * @throws Error where the origin exposes no `crypto.subtle` — see `subtleCrypto.ts`. Nothing is
 * invented in that case: a fingerprint that could not be computed must not read as one that was.
 */
export async function deriveHostKeyFingerprint(spkiDer: Uint8Array): Promise<string> {
  const digest = await requireSubtleCrypto().digest("SHA-256", spkiDer as BufferSource);
  return `SHA256:${base64NoPad(new Uint8Array(digest))}`;
}
