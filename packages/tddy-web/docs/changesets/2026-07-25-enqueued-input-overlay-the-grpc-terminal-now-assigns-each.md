# 2026-07-25 — enqueued-input-overlay: the gRPC terminal now assigns each sent chunk a cumulative byte offset (`GrpcSessionTerminal` sends `input_offset = BigInt(enqueue(data))`) and consumes `SessionTerminalOutput.acked_input_offset`. New `lib/terminalInputQueue.ts` (`classifyInput` keyboard/mouse/control + `TerminalInputQueue` offset accounting, ack-trim, `overlayModel` coalescing/overflow), `components/sessions/useEnqueuedInput.ts` (owns the queue + 500 ms reveal timer), and `components/connection/EnqueuedInputOverlay.tsx` (single-line overlay: text inline, `🖱×n` mouse run, `⋯+n` overflow). Feature [enqueued-input-overlay.md](../../../../docs/ft/web/enqueued-input-overlay.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-web)

**Type:** Feature


