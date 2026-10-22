# 2026-10-22 — a terminal exposure holds its own session's gate, not the registry

**Type:** Fix

`#satellite` 16/16, the sweep ([#695](https://github.com/uppin/tddy-coder/pull/695)); cross-package entry:
[2026-10-22-satellite-16-sweep.md](../../../../docs/dev/changesets/2026-10-22-satellite-16-sweep.md). Behaviour-preserving unless a line says otherwise; production lines are counted to the first
`#[cfg(test)]`.

- `ensure_exposed_terminal` is single-flight **per session**: each recorded terminal carries its own gate,
  held across the join, and the registry lock is taken only to read and write the record. A slow join to
  one session no longer holds up every other session's join and every other registry access.
- Recording a terminal again under the same id waits for a reach in flight and keeps its gate, and a reach
  writes its exposure back only to the record it was gated by — so a relaunch is exposed afresh rather
  than answered by the old PTY's exposure.
- Tests: `two_sessions_are_exposed_at_the_same_time` (red against the registry-wide lock) and
  `a_terminal_recorded_again_while_a_reach_is_in_flight_is_exposed_afresh`.
- Resolves the backlog entry `2026-10-19-terminal-exposure-lock-spans-the-join`.
