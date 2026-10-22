# oversized-file: service/rpc.rs — the activity service's whole `#[async_trait]` impl

**Location:** `packages/tddy-session-activity/src/service/rpc.rs`
**Category:** oversized-file
**Detected:** 2026-10-22 by `#satellite` 16/16 (the sweep), while splitting `service.rs` (835 production lines, over budget since `#635` grew it by 16)
**Metrics:** **509 production lines** (2026-10-22) · budget 500 · ~1.02× over · the impl alone is 481 lines
**Thresholds breached:** length 509 > 500
**Restructure:** required — but not by the engine: a trait impl cannot be split across files (`E0119`), and `extract_method` is refused inside an `#[async_trait]` impl (`docs/dev/todo/2026-10-19-restructure-extract-method-refused-under-async-trait.md`). The sweep moved the whole impl out of `service.rs` (835 → 201) and the new file lands nine lines over.
**Status:** Open — introduced by the sweep, which could not take it lower without the hand move below, which the developer has not consented to

## What would close it

The `#653` precedent: each trait method's body into an inherent `*_impl` method in a sibling module
(`service/handlers.rs`), the trait method delegating. That is a hand move (the engine cannot reach the
bodies), and it also gives `report_agent_activity` (131 lines) and `stream_acp_replay` (80 lines, recorded in
`complexity-service-stream-acp-replay.md`) a place to be cut.
