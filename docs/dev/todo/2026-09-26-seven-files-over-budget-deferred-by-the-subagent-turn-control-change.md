# 2026-09-26 — Seven files over the 500-line budget, six grown by one change and deferred

**Category:** Deferred work
**Source:** `2026-09-26-subagent-turn-control-and-honest-tool-failure` changeset, PR #545 —
`/pr-wrap` step 3.5 file-length gate

The gate rates "already ≥ 500 and this PR grew it further" as **decompose now**, and deferring
growth a PR caused needs explicit developer consent recorded here. Consent was given on
2026-09-26.

| File | was → now | Δ | Record |
|---|---|---|---|
| `packages/tddy-tools/src/server.rs` | 2,488 → 2,652 | +164 | `oversized-file-server.md` |
| `packages/tddy-sandbox-runner/src/runner.rs` | 2,610 → 2,611 | +1 | `oversized-file-runner.md` (new) |
| `packages/tddy-discovery/src/subagent.rs` | 1,208 → 1,431 | +223 | `oversized-file-subagent.md` |
| `packages/tddy-session-agents/src/service.rs` | 877 → 1,055 | +178 | `oversized-file-service.md` (new) |
| `packages/tddy-daemon-sandbox/src/workspace_tool_sandbox.rs` | 590 → 614 | +24 | `oversized-file-workspace-tool-sandbox.md` |
| `packages/tddy-discovery/src/subagent_runtime.rs` | 557 → 595 | +38 | `oversized-file-subagent-runtime.md` |
| `packages/tddy-tool-engine/src/lib.rs` | 793 → **789** | **−4** | `oversized-file-lib.md` — improved, alert-only |

## Why it was deferred

The change is one behaviour change across eight crates, already 58 files and +6,941 lines. The two
largest seams — `server.rs`'s subagent tool surface (~1,000 lines) and `subagent.rs`'s remaining
two extractions — would each add a mechanical diff comparable to the behaviour change and bury it
under a rename cascade. That is the trade the changeset's own *Decisions & Trade-offs* already
rejected once at planning time, on estimates; these are the measured numbers and the answer did
not change.

`runner.rs` is the clearest case: **+1 line, both of it comment**. It is a 2,611-line file that
this change did not meaningfully touch, and it should not pay for a decomposition it did not cause.

## What is actually owed, in rough order of value

1. **`tddy-tools/src/server.rs`** — the subagent tool surface (six tools' declarations, schema
   builders, handlers, the router and its roster gate) is contiguous and would take out ~1,000
   lines. This is the third change in two months to edit that block, so it pays back fastest.
2. **`tddy-session-agents/src/service.rs`** — the conversation half (four RPCs plus the shared
   turn path) is ~500 lines and leaves the roster half near 550, so it needs two cuts.
3. **`tddy-discovery/src/subagent.rs`** — `src/subagent/` already exists and absorbed two modules
   in PR #545; its record restates the arithmetic for the remaining extractions.
4. **`tddy-discovery/src/subagent_runtime.rs`** — one `PendingTurns` extraction (~170 lines) takes
   it to roughly 425, under budget outright. The cheapest of the set.
5. **`tddy-daemon-sandbox/src/workspace_tool_sandbox.rs`** — has a designed `extract_module` seam
   that passes a plain `check`; its line numbers need re-deriving with `restructure anchors`.
6. **`tddy-sandbox-runner/src/runner.rs`** — no seam designed yet; measure before planning.

Items 4 and 5 are small enough to be worth doing on their own, without waiting for a dedicated
effort.

## A gate bug found while running it

The script in `.agents/commands/pr-wrap.md` § 3.5 counts production lines with
`awk '/^[[:space:]]*#\[cfg\(.*test[),]/ {exit}'`, which stops at the **first** `#[cfg(test)]`
anywhere — including one nested inside a function body. `server.rs` has such an attribute inside
`permission_relay_socket_path` at `:39`, so the gate scored it **38** production lines instead of
2,488: a silently green gate on the largest file in the diff.

Anchoring the pattern at column 0 (`/^#\[cfg\(.*test[),]/`) fixes it, and matches how the existing
records were measured by hand. **Not fixed here** — it is an edit to the command, not to this
change's code, and it deserves its own diff so it is visible.
