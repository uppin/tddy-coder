# Terminal OSC 52 clipboard writes

Product behaviour: [Web terminal](../../../docs/ft/web/web-terminal.md) § Clipboard writes from the terminal (OSC 52).

## Shape

- **`src/lib/osc52.ts`** — `createOsc52Recognizer(onCopy, { maxDecodedBytes? })` returns a function fed every chunk (`Uint8Array | string`) a terminal is written. It is a byte-wise state machine (ground, escape, osc, osc-escape) that survives chunk boundaries at any byte, ends an OSC at BEL or `ESC \`, aborts it on CAN, SUB or a stray ESC, and calls `onCopy(text)` for every accepted `52;<selection>;<base64>` body. `OSC52_MAX_DECODED_BYTES` (1 MiB) is the default cap.
- **`src/components/GhosttyTerminal.tsx`** — one recognizer per terminal instance (`useMemo`), whose `onCopy` is `copyToClipboard` from `src/lib/clipboard.ts`. It is a **tap, not a filter**: `term.write` receives the same bytes as before, and the recognizer is fed the same bytes beside it, in the imperative `write`, and for `initialContent` — which is written (and tapped) **once**, by the effect that runs when the terminal becomes ready, so an OSC 52 inside it copies once. ghostty-web 0.4 has no OSC 52 hook of its own (its `copyToClipboard` serves mouse selection), so the copy happens only here.

## Rules

- Write-only. `?` is ignored and nothing is ever sent back to the program.
- Accepted selections are empty and `c`. Others (`p`, `s`, cut buffers) are ignored: no primary selection.
- Dropped silently: empty payload (it would blank the clipboard), invalid base64, over the cap, and bytes that are not valid UTF-8 (decoding lossily would put replacement characters on the clipboard). A body longer than the cap's base64 size is discarded while it streams in, so a hostile stream holds a bounded buffer.

## Tests

`src/lib/osc52.test.ts` (bun): terminators, selections, every byte split, multibyte UTF-8, read, invalid base64, over-cap, unrelated OSC, two sequences in one chunk. `cypress/component/GhosttyTerminalOsc52Acceptance.cy.tsx` feeds bytes into a mounted terminal with `navigator.clipboard.writeText` stubbed: agent terminal, bash tab, read writes nothing and sends no input, surrounding output still renders.

## Known limits

- The Tauri webview's and a plain-http origin's clipboard reachability is unverified: see `docs/dev/todo/2026-11-02-osc52-clipboard-reachability-unverified.md`.
