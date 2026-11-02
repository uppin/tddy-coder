/**
 * OSC 52 clipboard writes in a terminal's output stream.
 *
 * A program that wants the terminal to put text on the clipboard — tmux copy-mode, Neovim over
 * OSC 52, Claude Code's copy — prints `ESC ] 52 ; <selection> ; <base64> BEL` (or `ESC \` instead of
 * `BEL`). The recognizer taps a terminal's output, chunk by chunk, and hands the decoded text of
 * every such write to `onCopy`; the bytes themselves still go to the emulator unchanged. A
 * sequence may be split across any number of chunks.
 *
 * Only writes are honoured: a read (`?` as the payload) is ignored and never answered, so a program
 * cannot read the viewer's clipboard. So is an invalid payload, and one that decodes to more than
 * `maxDecodedBytes`.
 *
 * Reference: packages/tddy-web/docs/terminal-osc52-clipboard.md (`#session-tabs 4/7`)
 */

/** The largest decoded payload a recognizer copies: a bound on what a hostile stream can make us hold. */
export const OSC52_MAX_DECODED_BYTES = 1024 * 1024;

export interface Osc52RecognizerOptions {
  /** Payloads decoding to more bytes than this are dropped. Default `OSC52_MAX_DECODED_BYTES`. */
  maxDecodedBytes?: number;
}

/** A recognizer over one terminal's output: feed it every chunk the terminal is written. */
export type Osc52Recognizer = (chunk: Uint8Array | string) => void;

const ESC = 0x1b;
const BEL = 0x07;
const CAN = 0x18;
const SUB = 0x1a;
const OPEN_BRACKET = 0x5d; // ']' after ESC opens an OSC
const BACKSLASH = 0x5c; // ESC '\' is the string terminator (ST)

/** Room for the `52;<selection>;` header on top of the base64 payload. */
const HEADER_SLACK_BYTES = 16;

const BASE64_PAYLOAD = /^[A-Za-z0-9+/]*={0,2}$/;

type ParserState = "ground" | "escape" | "osc" | "oscEscape";

export function createOsc52Recognizer(
  onCopy: (text: string) => void,
  options: Osc52RecognizerOptions = {},
): Osc52Recognizer {
  const maxDecodedBytes = options.maxDecodedBytes ?? OSC52_MAX_DECODED_BYTES;
  // Base64 expands 3 bytes to 4 characters; a body longer than this cannot hold a copyable payload,
  // so it is discarded while it streams in rather than accumulated.
  const bodyCapBytes = 4 * Math.ceil(maxDecodedBytes / 3) + HEADER_SLACK_BYTES;
  const encoder = new TextEncoder();

  let state: ParserState = "ground";
  let body: number[] = [];
  let overflowed = false;

  function startOsc(): void {
    state = "osc";
    body = [];
    overflowed = false;
  }

  function abandonOsc(): void {
    state = "ground";
    body = [];
    overflowed = false;
  }

  function collect(byte: number): void {
    if (overflowed) return;
    if (body.length >= bodyCapBytes) {
      overflowed = true;
      body = [];
      return;
    }
    body.push(byte);
  }

  function endOsc(): void {
    const complete = !overflowed;
    const bytes = body;
    abandonOsc();
    if (complete) handleOscBody(bytes);
  }

  function handleOscBody(bytes: number[]): void {
    let text = "";
    for (const byte of bytes) text += String.fromCharCode(byte);
    if (!text.startsWith("52;")) return;
    const rest = text.slice("52;".length);
    const separator = rest.indexOf(";");
    if (separator < 0) return;
    const selection = rest.slice(0, separator);
    if (selection !== "" && selection !== "c") return;
    const copied = decodeWritePayload(rest.slice(separator + 1), maxDecodedBytes);
    if (copied !== null) onCopy(copied);
  }

  function step(byte: number): void {
    switch (state) {
      case "ground":
        if (byte === ESC) state = "escape";
        return;
      case "escape":
        if (byte === OPEN_BRACKET) startOsc();
        else if (byte === ESC) state = "escape";
        else state = "ground";
        return;
      case "osc":
        if (byte === BEL) endOsc();
        else if (byte === ESC) state = "oscEscape";
        else if (byte === CAN || byte === SUB) abandonOsc();
        else collect(byte);
        return;
      case "oscEscape":
        if (byte === BACKSLASH) {
          endOsc();
          return;
        }
        // An ESC that is not followed by '\' aborts the OSC and starts a new escape sequence.
        abandonOsc();
        state = "escape";
        step(byte);
        return;
    }
  }

  return (chunk) => {
    const bytes = typeof chunk === "string" ? encoder.encode(chunk) : chunk;
    for (const byte of bytes) step(byte);
  };
}

/**
 * The text of a write's base64 payload, or `null` when it is not a copy: a read (`?`), an empty
 * payload (which would blank the clipboard), invalid base64, a size over `maxDecodedBytes`, or
 * bytes that are not UTF-8.
 */
function decodeWritePayload(payload: string, maxDecodedBytes: number): string | null {
  if (payload.length === 0 || payload === "?") return null;
  if (payload.length % 4 !== 0 || !BASE64_PAYLOAD.test(payload)) return null;
  const padding = payload.endsWith("==") ? 2 : payload.endsWith("=") ? 1 : 0;
  const decodedBytes = (payload.length / 4) * 3 - padding;
  if (decodedBytes > maxDecodedBytes) return null;
  const binary = atob(payload);
  const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0));
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    // Not UTF-8: copying it would put replacement characters on the clipboard, so it is dropped.
    return null;
  }
}
