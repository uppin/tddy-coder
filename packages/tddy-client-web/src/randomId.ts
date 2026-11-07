/**
 * Random id generation that also works outside a secure context.
 *
 * `crypto.randomUUID` is exposed only on secure origins (https, or localhost/127.0.0.1), so
 * tddy-web served over plain http on a LAN address has `crypto` but not `randomUUID` — calling it
 * throws `crypto.randomUUID is not a function` and aborts whatever needed the id. Prefer the native
 * implementation, fall back to `crypto.getRandomValues` (available on insecure origins too), and
 * only then to `Math.random`.
 */

/** Formats 16 bytes as a hyphenated RFC 4122 v4 UUID string. */
function formatUuidV4(bytes: Uint8Array): string {
  bytes[6] = (bytes[6] & 0x0f) | 0x40; // version 4
  bytes[8] = (bytes[8] & 0x3f) | 0x80; // variant 1
  const hex = Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
  return [
    hex.slice(0, 8),
    hex.slice(8, 12),
    hex.slice(12, 16),
    hex.slice(16, 20),
    hex.slice(20, 32),
  ].join("-");
}

/** A v4 UUID string. Never throws, whatever the origin's `crypto` support. */
export function randomUuid(): string {
  const webCrypto = typeof crypto !== "undefined" ? crypto : undefined;
  if (typeof webCrypto?.randomUUID === "function") {
    return webCrypto.randomUUID();
  }

  const bytes = new Uint8Array(16);
  if (typeof webCrypto?.getRandomValues === "function") {
    webCrypto.getRandomValues(bytes);
  } else {
    for (let i = 0; i < bytes.length; i += 1) {
      bytes[i] = Math.floor(Math.random() * 256);
    }
  }
  return formatUuidV4(bytes);
}
