# oversized-file: session_coordinate_handlers.rs

**Location:** `packages/tddy-session-lifecycle/src/connection_service/session_coordinate_handlers.rs`
**Category:** oversized-file
**Detected:** 2026-09-19 — `/pr-wrap` step 3.5 file-length gate
**Metrics:** **818 production lines** · budget 500
**Thresholds breached:** length 818 > 500
**Restructure:** two successive two-brace impl splits + `extract_module --to_file` — designed, not applied
**Status:** Open — **unclaimed**

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-19 | 818 | 817 → 818 in this PR — a **one-line** change; the file was already 1.6× over |
| 2026-09-24 | 818 | touched by #509 (`#keyring` 2/9) and **unchanged by it**: 818 on the merge-base with `origin/master` (`4e7157d2`) and HEAD. Five `let os_user = self` → `&self` edits (`os_user_for_github` now borrows from the live `users:` holder), no lines added. The file has no `#[cfg(test)]`, so the count is its whole length |

## What would close it — designed seam

`SESSION_SERVICE` const, `bridge_conn_resume_response` fn, one `impl DaemonSessionHost` with 8
handlers. Unlike `svc_spawn_split_agent.rs`, **both blocks worth moving are leading blocks**, so
`anchors --items "impl DaemonSessionHost"` addresses each directly — no derived range needed.

| Step | Split after | Block extracted | Parent after |
|---|---|---|---|
| 1 | `connect_session_at_session_coordinate` (L269) | `list_sessions` + `start_session` + `connect_session`, ~230 | ~588 |
| 2 | `resume_session_at_session_coordinate` (L513) | `resume_session`, ~243 | **~345** |

## Constraints a later session must know

- No handler calls another in this impl — every caller is in `svc_session_lifecycle_ports.rs` and
  `agent_roster.rs` via `self.`, so there are no module-path references to fix.
- `resume_session` calls module-level `bridge_conn_resume_response` (L293, L308). The import pass
  should restore that as `super::…` since a child module sees the parent's private items — verify
  on `check --deep`.
- `places_of` first-match applies to the second split as it does everywhere: after step 1 there are
  two `impl DaemonSessionHost` items and only the leading one is addressable by name.
