# Changeset: Terminal file drop actually delivers the path

**PRD**: `docs/ft/web/web-terminal.md` § File drop upload
**Branch**: `master`

Two independent bugs both ended in "drop a file, nothing happens". The first threw before any chunk
was sent; with it fixed, the second stalled the upload mid-file.

## Problem 1 — `crypto.randomUUID` is secure-context only

Dragging a file onto the Ghostty terminal in tddy-web did nothing — no upload, no path typed. The
console showed:

```
useSessionFileUpload.ts:49 Uncaught (in promise) TypeError: crypto.randomUUID is not a function
    at useSessionFileUpload.ts:49:31
    at t4 (TerminalFileDropZone.tsx:47:12)
```

`crypto.randomUUID` is a **secure-context-only** API: it exists on `https://` and on
`localhost`/`127.0.0.1`, but not when tddy-web is served over plain `http://` on a LAN address (the
usual way the daemon's web bundle is reached). `uploadFiles()` minted the per-drop `upload_id` with
it as its very first statement, so the whole drop handler threw before any chunk was sent — the
progress store never started and no path was typed. `crypto.getRandomValues` **is** available on
insecure origins, so only the UUID convenience wrapper is missing.

The identical failure applies to the mobile `TerminalUploadButton`, which shares the hook.

## Problem 2 — a 256 KiB chunk is a multi-frame LiveKit message, and a lost frame wedges the RPC

With problem 1 fixed the upload started but stopped mid-file, silently: no path typed, no error, the
progress bar never finished. Evidence from a real drop of a 1.5 MB+ PNG (dev daemon log
`tmp/logs/daemon`, session `019f95c7-…`):

- The daemon received exactly **six** `UploadSessionFileChunk` calls (`request_id` 116, 119, 122,
  125, 128, 133), then none. The stored file is exactly `6 × 262144` bytes and has no PNG `IEND` —
  truncated at a chunk boundary.
- Inter-chunk latency grew monotonically: 2.1 s → 4.7 s → 4.0 s → 5.4 s → 7.1 s (~64 KB/s), i.e. the
  data channel was congesting.
- `request_id` **135–137 never arrived**, while `ListSessions` (`134`, then `138`, `139`, … every
  2 s) kept arriving throughout. The channel was healthy; only the big request vanished.

`UPLOAD_CHUNK_SIZE` was 256 KiB, so each upload request was ~262,599 bytes — over the LiveKit data
packet budget (`MAX_CHUNK_FRAME_BYTES` = 60,000), hence split into 5 chunk frames by
`packages/tddy-livekit-web/src/chunking.ts`. `ChunkReassembler` (Rust side, `chunking.rs`) holds
partial messages in `pending` with **no timeout and no eviction**, so one dropped frame means the
request is never delivered, never answered, and never fails.

And `LiveKitTransport.unary` accepted `_timeoutMs` and **ignored it** — no deadline anywhere on the
LiveKit path — so the browser awaited that response forever. `uploadFiles` never reached
`insertInput`, and because nothing threw, not even the `failFile` error strip appeared. Exactly the
reported symptom: "no error, but no file reference".

## Checklist

- [x] Write unit tests (red first)
- [x] `randomUuid()` helper with insecure-origin fallback
- [x] Use it for the per-drop `upload_id`
- [x] `LiveKitTransport.unary` honours `timeoutMs` (rejects `DeadlineExceeded`)
- [x] Upload chunk sized to fit one LiveKit data packet
- [x] Per-chunk deadline so a lost chunk fails the file instead of hanging
- [x] Regression-run the upload Cypress specs

## Files to create

| File | Purpose |
|------|---------|
| `packages/tddy-web/src/lib/randomId.ts` | `randomUuid()` — native `crypto.randomUUID` when present, else an RFC 4122 v4 built from `crypto.getRandomValues`, else `Math.random` |
| `packages/tddy-web/src/lib/randomId.test.ts` | Units: v4 shape + uniqueness on a secure origin, on an insecure origin (`randomUUID` deleted), and with no `crypto` global at all; id stays a safe single path segment (the daemon rejects an `upload_id` that is not a basename) |

## Files to modify

| File | Change |
|------|--------|
| `packages/tddy-web/src/hooks/useSessionFileUpload.ts` | `crypto.randomUUID()` → `randomUuid()` for the per-drop `upload_id`; `useDaemonUploadChunk` sends each chunk with `{ timeoutMs }` (new `UPLOAD_CHUNK_TIMEOUT_MS` = 20 s, overridable per call) |
| `packages/tddy-web/src/lib/fileUploadChunks.ts` | `UPLOAD_CHUNK_SIZE` 256 KiB → **48 KiB**; new `UPLOAD_REQUEST_ENVELOPE_HEADROOM` (8 KiB) documenting the non-data part of the request |
| `packages/tddy-web/src/lib/fileUploadChunks.test.ts` | Pins the new size and that `UPLOAD_CHUNK_SIZE + headroom ≤ MAX_CHUNK_FRAME_BYTES` (imported from `tddy-livekit-web`, so the two can't drift apart silently) |
| `packages/tddy-livekit-web/src/transport.ts` | `unary` honours the `timeoutMs` Connect call option: a deadline timer clears the pending entry and rejects with `ConnectError(Code.DeadlineExceeded)`; cleared on response, error, and abort. No timeout passed ⇒ previous indefinite behaviour |
| `packages/tddy-livekit-web/src/index.ts` | Export `MAX_CHUNK_FRAME_BYTES` so callers can size their payloads to one packet |
| `docs/ft/web/web-terminal.md` | New flow rule 7: a stalled chunk fails the file rather than leaving the drop pending |

## Design decisions

- **A shared `lib/` helper, not an inline try/catch.** Any future per-client id (drop ids, request
  ids) hits the same secure-context trap; one audited helper keeps the fallback in one place.
- **Keep the v4 UUID shape** rather than switching to a shorter random token: `upload_id` becomes a
  directory name under `{session_dir}/uploads/`, and the existing daemon tests and docs describe it
  as a UUID.
- **No secure-context feature detection at the call site.** The helper degrades silently because a
  drop upload does not need cryptographically strong ids — it needs collision-free ones.
- **Shrink the upload chunk rather than "fix" the chunking codec.** Single-packet requests are the
  proven-reliable shape on this transport (terminal I/O and every other RPC are single-packet and ran
  for hours through the same congestion window that lost the upload). 48 KiB keeps the whole request
  in one packet with ~11 KB of slack, and the hook already awaits chunks serially, so uploads apply
  their own backpressure. Round trips go up (32 for a 1.5 MB file) but the previous run managed only
  ~64 KB/s before stalling, so this is not a throughput regression.
- **Deadline in the transport, not a retry loop.** The transport can only tell "lost" from "slow"
  with a deadline, and a retry would need idempotency the append-based upload RPC does not have
  (a retried chunk would append twice). Failing the file is the honest outcome, and the changeset's
  existing skip-and-report behaviour already covers it.
- **Timeouts stay opt-in per call.** Defaulting every LiveKit unary to a deadline would break
  legitimately slow RPCs (`StartSession` does remote git work). Only the upload opts in for now.

## Known remaining gap (not fixed here)

`publishRequest` fires every frame of a message at `publishData` **without awaiting** it, so an
oversized message still bursts N frames with no backpressure — the condition under which this drop
was lost. Uploads no longer produce oversized messages, but other chunked payloads still can (large
`StreamSessionActivity` snapshots, ACP replay), and none of them pass a `timeoutMs`, so a lost frame
there still wedges that call silently. Fixing it properly means awaiting `publishData` per frame
(real SCTP backpressure) and/or an eviction/nack policy in `ChunkReassembler`. Both touch the shared
terminal I/O hot path, so they are left for a deliberate follow-up rather than folded in here.

## Tests

- Unit `packages/tddy-web/src/lib/randomId.test.ts` — 6 pass.
- Unit `packages/tddy-web/src/lib/fileUploadChunks.test.ts` — 5 pass (adds the single-packet bound).
- Unit `packages/tddy-livekit-web/src/transport.test.ts` — 8 pass (+3: deadline fires, prompt response
  is unaffected, no-timeout callers still never time out). The deadline test fails before the fix by
  hanging until the runner's own 5 s timeout — the exact production symptom.
- Whole `tddy-web` + `tddy-livekit-web` unit suite: 550 pass, 0 fail.
- Cypress regression `TerminalFileDropUpload` 4, `TerminalFileUploadFailure` 2,
  `MobileTerminalUploadButton` 2, `TerminalFileUploadProgressFooter` 2 — 10 pass, 0 fail.
- **Verified end-to-end over a live LiveKit data channel** (LAN browser → dev daemon, claude-cli
  session `019f95c7-…`, 1.99 MB PNG): 41 chunk RPCs, `write_upload_chunk: completed …png` at
  `21:03:31.987`, `SendTerminalInput` (577 B ⇒ ~159 B payload = the quoted path) at `21:03:32.256`,
  and the session's PTY capture buffer ends with the path in Claude Code's prompt. The PTY's winsize
  (54×100, not the 24×220 spawn default) independently proves the control token and terminal lookup
  resolve for this session, so the input was written to the child's stdin.

## Remaining gap — the drop is slow enough to read as a failure

The verified drop above took **42 s** (`21:02:49.9` → `21:03:31.99`): 1.99 MB in 41 serial chunk
round trips, 0.23–4.97 s each, ~47 KB/s over the LiveKit data channel. During those 42 s the only
feedback is `UploadProgressIndicator` — a 64-px bar plus `1 file · N%` in the bottom host-stats
footer, far from the terminal the user is watching. Both reports of "the drop does nothing" after the
fixes above were this: the path did arrive, three quarters of a minute later. Two independent
follow-ups, neither done here:

- **Throughput.** Chunks are strictly serial because the daemon appends in arrival order. An explicit
  `offset` on `UploadSessionFileChunk` (write-at instead of append) would make arrival order
  irrelevant and let several chunks be in flight, turning an RTT-bound transfer into a
  bandwidth-bound one. Proto + daemon + hook change.
- **Feedback.** Progress belongs where the drop happened (an overlay on the terminal, or a typed
  placeholder replaced by the path on completion), not only in the screen footer.
