# oversized-file: codex_acp.rs

**Location:** `packages/tddy-agent-backend/src/backend/codex_acp.rs`
**Category:** oversized-file
**Detected:** 2026-09-23 — `/pr-wrap` step 3.5 file-length gate on #522 (`#carve` 12/14), production lines counted to the first `#[cfg(test)]`
**Metrics:** **562 production lines** of 591 total (`#[cfg(test)]` at line 563; 29 test lines) · budget 500 · **1.1× over**
**Thresholds breached:** length 562 > 500
**Restructure:** not designed
**Status:** Open — **unclaimed**
**Moved:** 2026-09-23 — from `packages/tddy-core/src/backend/codex_acp.rs` by `#carve` 12/14 (PR #522), with `git mv`
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured; the seam table is a first reading of the item list

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-23 | 562 | first detection, at the file's new home. The size is inherited from `tddy-core`, not grown: no line of the file changed |

## What would close it — candidate seams (not proven)

| Seam | Items | Out |
|---|---|---|
| A | the ACP client: `CodexAcpAccumulator`, `CodexAcpCommand`, `TddyCodexAcpClient` and its `impl Client` (L19–136) | ~120 |
| B | OAuth login helpers: `acp_error_suggests_retry_oauth`, `invoke_codex_oauth_login_blocking`, `run_oauth_login_via_codex_cli`, `resolve_codex_cli_for_oauth` (L137–176, L464–469) | ~45 |
| C | the worker: `run_codex_acp_worker`, `spawn_codex_acp_agent` (L177–445) — `run_codex_acp_worker` has its own record, `complexity-codex-acp-run-codex-acp-worker.md` | ~270 |

Seam A alone brings it to about 440 lines. Prove the seams with `restructure check --deep` before applying them.
