# 2026-11-11 — Cursor's headless conversations report no usage

**Category:** Deferred from `#usage-binding 3/5` (#892)
**Source:** `docs/dev/changesets/2026-11-10-usage-binding-live-tap.md`; `packages/tddy-coding-agent-specs/specs/cursor.yaml`

The live tap (`headless.usage_events`) lets an agent whose only usage source is its stream be bound
with an ordinary `jsonl` binding. Cursor was planned as its first user, but **no recorded Cursor
stream-json `result` carries usage** (`tddy-integration-tests/tests/cursor_backend.rs` records
`{"type":"result","result":…,"session_id":…}` only). A binding written without a real sample would be
invented field names, so `cursor.yaml` declares neither `usage_events` nor `usage:`.

**What would close it:** record a real `agent -p --output-format stream-json` run; if its `result`
(or another event) reports tokens or cost, add `headless.usage_events` and a
`{session_dir}/usage/{conversation_id}.jsonl` source to `cursor.yaml`, with a test in
`tddy-coding-agent-specs/tests/usage_section_acceptance.rs` over the recorded line. If it reports
none, delete this entry and say so in `session-token-accounting.md`.
