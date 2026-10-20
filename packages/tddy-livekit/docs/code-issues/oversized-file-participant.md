# oversized-file: participant.rs

**Location:** `packages/tddy-livekit/src/participant.rs`
**Category:** oversized-file
**Detected:** 2026-10-19 — `/pr-wrap` step 3.5 file-length gate (#satellite 8/13, #630)
**Metrics:** **907 production lines** (956 before #630) · budget 500 · **1.8× over** · see also `complexity-participant-run.md` (`run`, 198 lines)
**Status:** Open — **unclaimed**; alert only, this PR shrank the file (-49) by moving `merge_participant_metadata_json` to `tddy-changeset`

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-19 | 907 | 956 on the merge-base with `origin/master` → 907 in #630. Not grown. Dependent PRs in the `#satellite` stack touch this file, so the split waits for the stack to land |
