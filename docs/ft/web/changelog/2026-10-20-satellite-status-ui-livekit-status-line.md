# 2026-10-20 — The settings screen shows whether LiveKit is working

- The daemon settings screen shows a status line in its LiveKit block: **Off**, **Missing: <fields>**, **Connecting…**, **Connected · 3m** or **Unreachable · 12s: <error>**. It refreshes every few seconds and right after a save, so you can see whether the daemon reached the server you just entered, without reading logs or restarting. An older daemon shows no line.
