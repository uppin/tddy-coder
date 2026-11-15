# 2026-11-15 — Why a split session's agent exits is still unknown, and what the rest of the split fix still owes

**Category:** Known defect
**Source:** `docs/dev/todo/2026-11-15-split-session-agent-not-running.md` (deleted at the
#905 wrap, which closed its fixes 1 and 2). This entry records what is still open after
[#905](https://github.com/uppin/tddy-coder/pull/905) (`#split-alive` 1/4).

## Still open

1. **Fix 4 — why the Claude process exits.** Every split agent investigated on 2026-11-14/15
   died within seconds with no trace; the offline reproduction with the same flags and env
   did not exit. Candidates, in order: the real `tddy-tools --mcp` with the LiveKit env (the
   only part of the launch not reproduced offline), the daemon PTY's cleared env, and the
   long multi-line prompt. Since #905 the agent's terminal is recorded and its exit is
   logged, so the next failed split start reports the agent's own last output — read that
   first.
2. **Fixes 6–7 of the original entry landed in the rest of stack `#split-alive`:**
   fixes 6–7 (the start result's branch/worktree and the drawer's pairing) are done in
   [#908](https://github.com/uppin/tddy-coder/pull/908) (`#split-alive` 4/4). Fix 3 (the
   split agent's hooks load, with the split env carrying the spec's `run.env`) is done in
   [#906](https://github.com/uppin/tddy-coder/pull/906) (`#split-alive` 2/4), and fix 5
   (the project-home inspection tools are split-aware) is done in
   [#907](https://github.com/uppin/tddy-coder/pull/907) (`#split-alive` 3/4).
