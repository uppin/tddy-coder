# 2026-11-02 — `GhosttyTerminal` writes `initialContent` twice on mount

**Category:** Known defect (pre-existing)
**Source:** `#session-tabs 4/7` (#807), found while wiring the OSC 52 tap

`GhosttyTerminal` writes `initialContent` once at the end of its setup effect and again from the `[initialContent, ready]` effect once `ready` flips, so the emulator receives it twice. The OSC 52 tap sits beside both writes, so an OSC 52 sequence inside `initialContent` copies twice. The copy is idempotent, so it does no harm; the double write is the defect.

**Why deferred:** removing either write changes what every embedder of the component renders on mount (scrollback content, revision counting), which is outside the clipboard change and needs the terminal component specs as a baseline.

**Recommendation:** drop the write in the setup effect (the `ready` effect covers it), move the tap with it, and run the `GhosttyTerminal*` specs; do it with the `GhosttyTerminal.tsx` split recorded in `2026-10-24-ghostty-terminal-components-over-budget.md`.
