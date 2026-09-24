/**
 * The credential vault passphrase rule, as the daemon applies it — said first in the page so the
 * operator is not sent a refusal for something the page could have told them.
 *
 * These mirror `tddy_credentials::MIN_PASSPHRASE_CHARS` and `MAX_PASSPHRASE_CHARS`; the daemon is
 * the authority and refuses anything outside them regardless. Both count **characters** — Unicode
 * code points, as Rust's `str::chars` does — not the UTF-16 code units `String.length` counts, so a
 * passphrase of emoji is measured here exactly as the daemon measures it.
 */

/** The shortest passphrase a vault is created or reset under, in characters. */
export const MIN_PASSPHRASE_CHARS = 8;

/** The longest passphrase the daemon accepts, in characters. */
export const MAX_PASSPHRASE_CHARS = 1024;

/** How many characters (code points) `passphrase` holds. */
export function passphraseChars(passphrase: string): number {
  return Array.from(passphrase).length;
}

/** Whether a vault may be created or reset under `passphrase`. */
export function isAcceptableNewPassphrase(passphrase: string): boolean {
  const chars = passphraseChars(passphrase);
  return chars >= MIN_PASSPHRASE_CHARS && chars <= MAX_PASSPHRASE_CHARS;
}
