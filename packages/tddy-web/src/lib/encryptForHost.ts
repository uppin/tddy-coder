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
 */

/** RSA-OAEP with SHA-256, matching the daemon's `HostKeypair::decrypt`. */
const ALGORITHM = { name: "RSA-OAEP", hash: "SHA-256" } as const;

/**
 * Encrypt `plaintext` under `spkiDer` (the host's SPKI DER public key).
 *
 * @returns the ciphertext bytes to place in `AnswerHostPromptRequest.encryptedAnswer`.
 */
export async function encryptForHost(
  spkiDer: Uint8Array,
  plaintext: string,
): Promise<Uint8Array> {
  // TODO(agent-add-key): implement
  void spkiDer;
  void plaintext;
  throw new Error("agent-add-key: encryptForHost not implemented");
}
