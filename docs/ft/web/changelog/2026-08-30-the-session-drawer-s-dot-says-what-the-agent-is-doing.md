# 2026-08-30 — The session drawer's dot says what the agent is doing

- **Four states instead of three:** grey (gone), steady green (idle), **blinking green** (activity within 30 seconds, a fade in and out), yellow (attention required).
- **Fed by one daemon-level subscription** for the whole drawer, not one per row — previously the only live activity feed opened for the focused session alone, so a drawer of twelve sessions showed nothing about eleven of them.
- **Viewing a session settles its dot**, and later activity raises it again. Reaching a session by deep link or Back does not settle it — a reload should not clear an indicator nobody looked at.
- **A pending elicitation stays yellow until it is answered**, because clearing it on a glance would claim the operator had dealt with an open gate they had not.
- **The blink respects `prefers-reduced-motion`**, and stops on its own once activity ages out, so a session whose agent died mid-turn stops claiming to be working.
- **The expanded list and the collapsed strip now share one `SessionIndicatorDot`** — two copies of that dot is how the strip previously came to lack a state the list had.
- See [session-drawer.md](../session-drawer.md).
