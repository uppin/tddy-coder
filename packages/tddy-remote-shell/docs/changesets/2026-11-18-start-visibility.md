# 2026-11-18 — A start's calls on its target are recorded, and a timeout names its command

**Type:** Feature

- `RecordedShell::new(inner, sink)` runs every call through `inner` and tells its `RemoteCallSink`
  a `RemoteCallReport` when an `exec` ends: address, the caller's command (never a transport's
  wrapper), duration, `RemoteCallOutcome` (`Exited { code }`, `TimedOut`, `Unreachable`) and a
  failed call's stderr tail. `exec_streamed` and `interactive` pass through unrecorded.
- `redact.rs`: `scrub_credentials` (a URL's user-info reads `***`), `command_head` (scrubbed, 200
  characters), a 400-character stderr tail — shared by the transports, the recorder and the start ledger.
- `RemoteShellError::TimedOut` carries `command`:
  `` <address>: `<command>` gave no answer within <after>; the command was stopped ``.

See [architecture.md § Recording calls](../architecture.md#recording-calls). Full record: [`docs/dev/changesets/2026-11-18-start-visibility.md`](../../../../docs/dev/changesets/2026-11-18-start-visibility.md).
