/**
 * Encrypting an answer for a host — and refusing to, where the browser cannot.
 *
 * `crypto.subtle` is **secure-context only**, and the daemon serves this bundle over plain `http://`
 * on a LAN address (`docs/insecure-origin-constraints.md`). Every Cypress run and every `localhost`
 * session is a secure context, so the origin that matters in production is the one no browser test
 * exercises — it is pinned here instead, by taking `subtle` away.
 */

import { afterEach, describe, expect, it } from "bun:test";
import { encryptForHost } from "./encryptForHost";

const PASSPHRASE = "correct horse battery staple";

const realCrypto = globalThis.crypto;

/** Swaps the `crypto` global, the way `randomId.test.ts` does, so an insecure origin is reachable. */
function setCrypto(value: unknown) {
  Object.defineProperty(globalThis, "crypto", { value, configurable: true, writable: true });
}

/** A `crypto` with everything an insecure origin keeps, and nothing it withholds. */
function anInsecureOriginCrypto(): Crypto {
  return { getRandomValues: realCrypto.getRandomValues.bind(realCrypto) } as unknown as Crypto;
}

/** A 2048-bit RSA-OAEP(SHA-256) public key in SPKI DER — what the daemon publishes with a prompt. */
async function anRsaOaepPublicKey(): Promise<Uint8Array> {
  const keyPair = await realCrypto.subtle.generateKey(
    {
      name: "RSA-OAEP",
      modulusLength: 2048,
      publicExponent: new Uint8Array([0x01, 0x00, 0x01]),
      hash: "SHA-256",
    },
    true,
    ["encrypt", "decrypt"],
  );
  return new Uint8Array(await realCrypto.subtle.exportKey("spki", keyPair.publicKey));
}

describe("encryptForHost", () => {
  afterEach(() => {
    setCrypto(realCrypto);
  });

  it("returns a key-sized ciphertext that does not carry the passphrase", async () => {
    // Given a host's published key
    const spkiDer = await anRsaOaepPublicKey();

    // When the answer is encrypted for it
    const ciphertext = await encryptForHost(spkiDer, PASSPHRASE);

    // Then a 2048-bit RSA-OAEP block comes back, and the plaintext is not in it
    expect(ciphertext.byteLength).toBe(256);
    expect(new TextDecoder().decode(ciphertext)).not.toContain(PASSPHRASE);
  });

  it("refuses to encrypt on an origin that exposes no crypto.subtle, saying why", async () => {
    // Given the origin the daemon actually serves this bundle on: plain http, so `crypto` is there
    // but `subtle` is not
    const spkiDer = await anRsaOaepPublicKey();
    setCrypto(anInsecureOriginCrypto());

    // When the dialog tries to encrypt the answer
    const encrypting = encryptForHost(spkiDer, PASSPHRASE);

    // Then it refuses out loud, naming the reason — matched as a pattern rather than as copy,
    // because what has to be true is that the operator is told the *origin* is why. A silent
    // fallback to plaintext here would send the passphrase to every peer in the common room.
    await expect(encrypting).rejects.toThrow(/secure context/i);
  });

  it("refuses to encrypt where the crypto global is absent entirely, saying why", async () => {
    // Given a browser that exposes no Web Crypto at all
    const spkiDer = await anRsaOaepPublicKey();
    setCrypto(undefined);

    // When the dialog tries to encrypt the answer
    const encrypting = encryptForHost(spkiDer, PASSPHRASE);

    // Then it refuses the same way, rather than throwing "cannot read properties of undefined"
    await expect(encrypting).rejects.toThrow(/secure context/i);
  });
});
