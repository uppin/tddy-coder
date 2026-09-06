# 2026-09-06 — From the 2026-09-06 `#optional-livekit` stack evaluation

**Category:** Future enhancement
**Source:** optional-livekit, PRs #437–#451

Findings from `/eval-changeset` over the integrated 9-PR stack (284 judged files, 26,577 changed
lines). The stack itself was healthy — **1.02× inflation**, 51% essential, 0% opportunistic — so
these are the design costs it *exposed*, not defects it introduced.

**Redesign — worth doing**

- **"Is LiveKit configured?" is decided in at least six independent places** — `daemon_settings.rs`,
  `common_room_supervisor.rs`, `session_room.rs`, `config.rs`, the web `clientConfig`, and
  `liveKitIsConfigured` in `packages/tddy-web/src/rpc/connections/localHost.ts`. **PR #449 exists
  entirely to reconcile them**: 20 production files, 1,590 changed lines, to honour one switch.
  Resolving availability **once** at daemon startup and serving it through the existing
  `ClientConfig` would have made #449 ≈4 files / ≈300 lines, and #451 (a session start that waited on
  LiveKit) most likely would not have existed at all — that wait is one of the same independent
  re-derivations. ~6 call sites to move, one config field, migratable incrementally.
  **Do this before the next "turn X off" switch**; the shape recurs and the modes can silently
  disagree at rest.
- **`packages/tddy-web/src/components/GhosttyTerminalSession.tsx` lands at 960 lines.** The terminal
  convergence removed a duplicate path (`GhosttyTerminalLiveKit` 736 + `GhosttyTerminalGrpc` 631) but
  produced one file above any comfortable review size. The cheapest extraction is the history/offset
  engine — the `terminalHistoryLoader` + `TerminalStreamOffset` orchestration — into a hook, leaving a
  presentational shell; no call sites move and no behaviour changes. Same family as the
  `SessionMainPane` / `SessionRuntime` entry below, and worth doing in the same pass.

**Considered and rejected** *(recorded so it is not re-proposed)*

- A **capability-aware surface registry** to centralise `useHasCapability`. The call sites are
  one-line guards at ~10 places; the indirection would cost more comprehension than it saves.

**Planning habits for the next stack** *(no code)*

- **Insert a node at its dependency-correct position, not at the tail.** Terminal convergence was
  added mid-planning and appended as position 5 when its dependencies allowed position 4. It then
  rewrote `SessionRuntime.tsx` wiring that the two PRs before it had just written — **239 lines
  reviewed twice, 81% of all rework in the stack**. When `/add-to-pr-stack` is used mid-planning,
  re-derive the topological position rather than appending.
- **Restack immediately after a predecessor is greened.** #451 sat four commits behind #449 —
  including #449's entire implementation — until the evaluation surfaced it, and conflicted on
  `liveKitSource.ts` and `selectedDaemon.tsx` when finally rebased. A `/pr-stack-rebase` at each green
  would have caught it while the diff was one commit old.

**Also observed, not owed to this stack**

- `useSelectedDaemon().room` being *ambient* is what made the migration cost 67 files / ~1,400
  incidental lines. That is now fixed by the connection model — noted only because the same "reachable
  from anywhere" shape elsewhere will cost the same on its next change.
