# 2026-11-08 — Terminals run on a TerminalEngine interface with top-based scrolling (#xterm 2/4, PR #867)

**Type:** Refactor

`tddy-terminal-web`'s terminal components reach their emulator only through a `TerminalEngine`
interface (`src/engine/TerminalEngine.ts`), implemented by `ghosttyEngine`, which now holds every
ghostty-web import and cast. Scroll positions count from the top of the buffer everywhere above the
engine, so the xterm.js engine that follows drops in without touching the components. No
user-visible behaviour changes; `tddy-session-web` and `tddy-web` specs read the scrollbar mirrors
instead of the removed `*ViewportY` probes. See
[tddy-terminal-web](../../../packages/tddy-terminal-web/docs/changesets/2026-11-08-terminal-engine-interface.md).
