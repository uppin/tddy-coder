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
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-02-terminal-osc52-clipboard.md (`#session-tabs 4/7`)
 */

/** The largest decoded payload a recognizer copies: a bound on what a hostile stream can make us hold. */
export const OSC52_MAX_DECODED_BYTES = 1024 * 1024;

export interface Osc52RecognizerOptions {
  /** Payloads decoding to more bytes than this are dropped. Default `OSC52_MAX_DECODED_BYTES`. */
  maxDecodedBytes?: number;
}

/** A recognizer over one terminal's output: feed it every chunk the terminal is written. */
export type Osc52Recognizer = (chunk: Uint8Array | string) => void;

export function createOsc52Recognizer(
  onCopy: (text: string) => void,
  options: Osc52RecognizerOptions = {},
): Osc52Recognizer {
  // TODO(osc52-clipboard): implement
  void onCopy;
  void options;
  return () => undefined;
}
