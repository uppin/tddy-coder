# SessionDrawer.tsx grew past 500 lines under `#project-home` 2/7

**Raised:** 2026-10-23 by `/pr-wrap` on #697
**Why deferred:** `SessionDrawer.tsx` went 453 → 534 lines with per-project grouping. `#project-home` 3/7 and 4/7 (#698, #699) also modify it, so a split inside the stack turns every one of them into a conflict.
**What to do:** after the stack lands, split per `packages/tddy-web/docs/code-issues/oversized-file-session-drawer.md` (group body component, collapse hook). Mechanical moves only; the drawer specs are the gate.
