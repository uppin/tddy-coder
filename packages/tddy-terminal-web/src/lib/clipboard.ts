/**
 * Clipboard write that also works outside a secure context.
 *
 * `navigator.clipboard` is exposed only on secure origins (https, or localhost/127.0.0.1), so
 * tddy-web served over plain http on a LAN address has no `navigator.clipboard` at all — see the
 * note in `randomId.ts`. Prefer the async Clipboard API, then fall back to a hidden `<textarea>`
 * plus `document.execCommand('copy')`, which works on insecure origins too.
 */

/** Copy `text` to the clipboard. Never throws; resolves whatever the origin's clipboard support. */
export async function copyToClipboard(text: string): Promise<void> {
  const clipboard =
    typeof navigator !== "undefined" ? navigator.clipboard : undefined;
  if (typeof clipboard?.writeText === "function") {
    try {
      await clipboard.writeText(text);
      return;
    } catch {
      // Fall through to the execCommand path (e.g. permission denied on some browsers).
    }
  }
  copyViaTextarea(text);
}

/** Legacy `execCommand('copy')` path for insecure origins where `navigator.clipboard` is absent. */
function copyViaTextarea(text: string): void {
  if (typeof document === "undefined") return;
  const textarea = document.createElement("textarea");
  textarea.value = text;
  textarea.setAttribute("readonly", "");
  textarea.style.position = "absolute";
  textarea.style.left = "-9999px";
  document.body.appendChild(textarea);
  textarea.select();
  try {
    document.execCommand("copy");
  } finally {
    document.body.removeChild(textarea);
  }
}
