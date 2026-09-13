# 2026-09-13 — `#unbundle` post-stack follow-ups (node 10)

**Stack:** `#unbundle` — node **10 of 10**, based on node 9 ([#481](https://github.com/uppin/tddy-coder/pull/481)).  
**PR:** [#482](https://github.com/uppin/tddy-coder/pull/482) — **all post-stack milestones land here** (single node, not M2…M7 follow-up PRs).

Closes stack-documented items from `docs/dev/todo/` that could not land on frozen predecessor nodes without cascade-stale rewrites.

## Responsibility

One PR closes every `#unbundle`-tagged todo grouped earlier as M1–M7. Track status in the table below; remove each todo file when its row is **Done** at `/pr-wrap`.

| Milestone | Theme | Status |
|-----------|--------|--------|
| M1 | LiveKit on local UDS; sandbox spawn argv grep | **Done** |
| M2 | Stdio transport switch; sandbox test relocation; in-jail suite; PTY relay; LiveKit test contention; supervisor spawn test split | Open |
| M3 | Restructure tool defects; versioned protos; stub eligible daemon; move-module clusters | Partial (stub eligible **Done**; host/worktree generated adapters **Done**) |
| M4 | Over-budget `service.rs` / spawn / auth-livekit splits; session-agent clone split | Open |
| M5 | Reflection descriptors; coder dispatch/terminal; permission NDJSON; session-activity tests | Partial (generated host/worktree adapters **Done**) |
| M6 | tddy-tui dep; proto crate split; boundary harness; ACP replay dedup; schema log targets; relay retire; exec-tool fixture | Partial (duplications **Done**; relay retire **Done**) |
| M7 | Desktop embed; LiveKit deadlines; vault atomic write; roster/web/agent product gaps; buildId; log targets; VNC | Open |

## Boundaries

- No edits to merged predecessor branches (nodes 1–9).
- Stack registration: 10 PRs, titles `#unbundle n/10`.

## Dependencies

- Node 9 wiring, node 6 `generate_tonic_adapter`, `tddy-daemon-livekit` service impl.

## Draft PR contract

- Base: `feature/unbundle/daemon-becomes-wiring`.
- Title: `fix(unbundle): post-stack follow-ups — local socket, todos, and stack hygiene (#unbundle 10/10)`.
- Verification: CI on PR #482; scoped local `./test -p` for touched packages during development.
