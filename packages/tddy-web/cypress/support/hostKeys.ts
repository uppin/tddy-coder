/**
 * Host prompt keys, as a spec states them: real key material, and the fingerprint a daemon would
 * publish beside it.
 *
 * The fingerprint is computed **here**, from the key's bytes, rather than through
 * `src/lib/hostKeyFingerprint.ts`. A fixture built by the code under test would agree with itself by
 * construction, and the property this node rests on is that the browser's fingerprint is the one
 * `packages/tddy-daemon/src/host_keypair.rs` publishes — SHA-256 over the SPKI DER, base64 with the
 * padding stripped, prefixed `SHA256:`. That is spelled out again below on purpose.
 */

/**
 * A host's prompt keypair: the half it publishes, and the half it keeps.
 *
 * The private half is what makes an answer *checkable*. "The ciphertext does not contain the
 * passphrase" is satisfied by base64, by a digest, by 256 random bytes — none of which any host
 * could read, and none of which is the claim. Only decrypting with the key the prompt published
 * states what the browser actually sent.
 */
export interface AHostPromptKeypair {
  /** The 2048-bit RSA-OAEP(SHA-256) SPKI DER a daemon publishes with a prompt. */
  spkiDer: Uint8Array;
  /** What an answer says, read the way only the host holding the private half can read it. */
  decrypt: (ciphertext: Uint8Array) => Promise<string>;
}

/**
 * A freshly generated host prompt keypair.
 *
 * Real key material, so a spec that submits an answer genuinely encrypts it. A stubbed
 * `crypto.subtle` would hollow out the one assertion carrying this node's security claim.
 */
export async function anRsaOaepKeypair(): Promise<AHostPromptKeypair> {
  const keyPair = await crypto.subtle.generateKey(
    {
      name: "RSA-OAEP",
      modulusLength: 2048,
      publicExponent: new Uint8Array([0x01, 0x00, 0x01]),
      hash: "SHA-256",
    },
    true,
    ["encrypt", "decrypt"],
  );
  return {
    spkiDer: new Uint8Array(await crypto.subtle.exportKey("spki", keyPair.publicKey)),
    decrypt: async (ciphertext: Uint8Array) =>
      new TextDecoder().decode(
        // Rethrown with the claim it was checking: WebCrypto reports a payload that is not a
        // ciphertext for this key as a bare `OperationError`, which names neither the key nor the
        // question.
        await crypto.subtle
          .decrypt({ name: "RSA-OAEP" }, keyPair.privateKey, ciphertext as BufferSource)
          .catch((reason: unknown) => {
            throw new Error(
              "this payload does not decrypt under the key the prompt published, so it is not " +
                `the answer encrypted for this host: ${String(reason)}`,
            );
          }),
      ),
  };
}

/**
 * A 2048-bit RSA-OAEP(SHA-256) public key in SPKI DER — the shape the daemon publishes.
 *
 * For the specs that only need the browser to encrypt something. A spec that has to prove *what*
 * was encrypted wants {@link anRsaOaepKeypair} instead, and keeps the private half.
 */
export async function anRsaOaepPublicKey(): Promise<Uint8Array> {
  return (await anRsaOaepKeypair()).spkiDer;
}

/** The fingerprint a host honestly advertises for `spkiDer`. */
export async function fingerprintOf(spkiDer: Uint8Array): Promise<string> {
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", spkiDer as BufferSource));
  const binary = Array.from(digest, (byte) => String.fromCharCode(byte)).join("");
  return `SHA256:${btoa(binary).replace(/=+$/, "")}`;
}

/**
 * What this browser has on record for a host's key.
 *
 * The storage key format is `hostKeyPinning.ts`'s contract with the browser, so it is written out
 * once here instead of in each spec — the same reason selectors live in a page object.
 */
export const hostKeyPins = {
  /** Put `fingerprint` on record for `hostId`, as an earlier answered prompt would have. */
  pin: (hostId: string, fingerprint: string) => {
    cy.window().then((win) => win.localStorage.setItem(`tddy.hostKeyPin.${hostId}`, fingerprint));
  },
  /** Assert what is now pinned for `hostId`. */
  expectPinned: (hostId: string, fingerprint: string) => {
    cy.window()
      .then((win) => win.localStorage.getItem(`tddy.hostKeyPin.${hostId}`))
      .should("equal", fingerprint);
  },
};
