/**
 * Encrypt an answer under a host's published public key, in the browser.
 *
 * `SubtleCrypto` is a platform API, so this adds **no dependency**. A passphrase is small — a
 * 2048-bit RSA-OAEP(SHA-256) payload carries ~190 bytes, comfortably more than any passphrase — so
 * plain OAEP suffices and no hybrid AEAD envelope is needed.
 *
 * What this buys: the LiveKit common room and any forwarding daemon stop seeing the plaintext. What
 * it does not buy on its own: protection from an *active* peer, since the public key arrives over
 * that same unauthenticated channel. See `hostKeyPinning` for the continuity check that bounds it.
 *
 * `SubtleCrypto` is also **secure-context only**, and the daemon serves this bundle over plain http
 * on a LAN address — so on the normal deployment there is nothing here to call. That is refused
 * with a stated reason and never worked around: the only fallback available would be sending the
 * passphrase in the clear, which is the exposure this module exists to remove.
 */

import { requireSubtleCrypto } from "./subtleCrypto";

/** RSA-OAEP with SHA-256, matching the daemon's `HostKeypair::decrypt`. */
const ALGORITHM = { name: "RSA-OAEP", hash: "SHA-256" } as const;

/**
 * Encrypt `plaintext` under `spkiDer` (the host's SPKI DER public key).
 *
 * @returns the ciphertext bytes to place in `AnswerHostPromptRequest.encryptedAnswer`.
 * @throws Error where the origin exposes no `crypto.subtle`, naming the origin as the cause.
 */
export async function encryptForHost(
  spkiDer: Uint8Array,
  plaintext: string,
): Promise<Uint8Array> {
  // Imported per call rather than cached: a `CryptoKey` outlives the prompt it belongs to, and the
  // import is cheap next to the guarantee that we always encrypt under the key just published.
  const subtle = requireSubtleCrypto();
  const publicKey = await subtle.importKey("spki", spkiDer as BufferSource, ALGORITHM, false, [
    "encrypt",
  ]);
  const ciphertext = await subtle.encrypt(
    { name: ALGORITHM.name },
    publicKey,
    new TextEncoder().encode(plaintext),
  );
  return new Uint8Array(ciphertext);
}
