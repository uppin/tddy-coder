/**
 * The one audited entry point to `crypto.subtle`, which tddy-web cannot assume it has.
 *
 * The daemon serves this bundle over plain `http://` on a LAN address, and that origin is **not a
 * secure context**: the browser exposes `crypto` but withholds `crypto.subtle` entirely.
 * `localhost` — every dev session and every Cypress run — *is* a secure context, so nothing local
 * can notice. See `docs/insecure-origin-constraints.md`, and the file-upload incident recorded
 * there, where `crypto.randomUUID` broke a whole feature on real devices while passing every test.
 *
 * Unlike `randomId.ts`, which degrades quietly on purpose, there is **no fallback** here and there
 * must not be: the only thing on the far side of `subtle` in this app is a passphrase being
 * encrypted for a host, and a plaintext fallback would hand it to every peer in the common room —
 * the exact exposure the encryption exists to remove. So callers refuse, loudly, with the reason.
 */

/**
 * Why nothing cryptographic can happen on this origin, in words an operator can act on.
 *
 * Names the cause rather than the symptom: "encryption failed" sends someone looking at the host,
 * while this sends them to the URL they typed, which is the thing they can change.
 */
export const NO_SUBTLE_CRYPTO_REASON =
  "this browser exposes no Web Crypto (crypto.subtle) on this page — browsers offer it only in a " +
  "secure context, and tddy is being served over plain http. Reach it over https, or over " +
  "localhost, to encrypt anything here";

/**
 * This origin's `SubtleCrypto`.
 *
 * @throws Error naming {@link NO_SUBTLE_CRYPTO_REASON} where the origin withholds it, so the caller
 * fails with a stated cause instead of a `TypeError` about a property of `undefined`.
 */
export function requireSubtleCrypto(): SubtleCrypto {
  const webCrypto = typeof crypto !== "undefined" ? crypto : undefined;
  const subtle = webCrypto?.subtle;
  if (subtle === undefined) {
    throw new Error(NO_SUBTLE_CRYPTO_REASON);
  }
  return subtle;
}
