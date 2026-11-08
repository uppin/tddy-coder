import { tddyDebug } from "tddy-client-web";
import type { GhosttyTerminalHandle } from "../terminal/types";

// `[tddy]` diagnostics for the terminal byte stream (enabled by the DEBUG mask).
// The 220-col garbling on reconnect lived here, so log incoming bytes / buffering / resize.
export const dTerm = tddyDebug("tddy:term:session");
export const dResize = tddyDebug("tddy:term:resize");
export const dHistory = tddyDebug("tddy:term:history");
export const dModes = tddyDebug("tddy:term:modes");
export const dChrome = tddyDebug("tddy:term:chrome");

/** Hex preview of the first `n` bytes for diagnosing garbled / misaligned output. */
export function hexPreview(data: Uint8Array, n = 24): string {
  return Array.from(data.slice(0, n), (b) => b.toString(16).padStart(2, "0")).join(" ");
}

/** DEC private mode set/reset sequences (`ESC [ ? Pm h|l`) in a chunk, e.g. `?1006h`, `?1049l`, and
 *  a hard reset (`ESC c`, reported as `RIS`) — the switches that decide whether the wheel goes to the
 *  TUI and whether the TUI owns navigation. Latin-1 decode keeps byte offsets. */
export function decModeChanges(data: Uint8Array): string[] {
  if (!data.includes(0x1b)) return [];
  const text = new TextDecoder("latin1").decode(data);
  return Array.from(text.matchAll(/\x1b\[\?([\d;]+)([hl])|\x1bc/g), (m) =>
    m[1] === undefined ? "RIS" : `?${m[1]}${m[2]}`,
  );
}

/** Whether the live terminal's program scrolls its own output: it is on the alternate screen, or it
 *  tracks the mouse. Mouse tracking counts on its own because a reattach learns it from the daemon's
 *  replayed mode prologue, which does not carry the alternate screen. */
export function tuiOwnsNavigation(term: GhosttyTerminalHandle): boolean {
  return (term.isAlternateScreen?.() ?? false) || (term.hasMouseTracking?.() ?? false);
}

/** `tddy:term:modes`: after a chunk is written, log the DEC modes it switched and the resulting
 *  mouse-tracking / alternate-screen state. */
export function traceModes(term: GhosttyTerminalHandle, modes: string[]): void {
  if (!dModes.enabled) return;
  dModes(
    "%s → tracking=%o altScreen=%o",
    modes.join(" "),
    term.hasMouseTracking?.() ?? false,
    term.isAlternateScreen?.() ?? false,
  );
}

/** Human-readable description of a terminal input byte sequence, for `debugLogging`. */
export function describeKey(bytes: Uint8Array): string {
  if (bytes.length === 1) {
    const b = bytes[0];
    if (b === 0x0d) return "Enter";
    if (b === 0x1b) return "Esc";
    if (b === 0x09) return "Tab";
    if (b === 0x7f) return "Backspace";
    if (b === 0x03) return "Ctrl+C";
    if (b < 0x20) return `Ctrl+${String.fromCharCode(b + 0x40)}`;
    if (b >= 0x20 && b < 0x7f) return `'${String.fromCharCode(b)}'`;
  }
  if (bytes.length === 3 && bytes[0] === 0x1b && bytes[1] === 0x5b) {
    const c = bytes[2];
    if (c === 0x41) return "Up";
    if (c === 0x42) return "Down";
    if (c === 0x43) return "Right";
    if (c === 0x44) return "Left";
  }
  if (bytes.length === 4 && bytes[0] === 0x1b && bytes[1] === 0x5b && bytes[3] === 0x7e) {
    if (bytes[2] === 0x35) return "PageUp";
    if (bytes[2] === 0x36) return "PageDown";
  }
  return `raw(${bytes.length})`;
}
