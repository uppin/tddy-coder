# 2026-10-25 — The terminal shows the right screen after a reconnect

A session terminal that connects, reconnects after a network blip or is remounted shows the current
screen immediately — no more blank or garbled screen until you resize the window. Every terminal
stream asks the host for a snapshot of the screen and then receives only new output. A screen
without terminal control still watches the terminal live; only its typing is refused: a keystroke the
host refuses because another screen controls the terminal shows **"Input not delivered"** until
typing is accepted again. See [Web terminal](../web-terminal.md).
