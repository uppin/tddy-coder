# 2026-10-18 — A new session's terminal no longer opens on a closed session's screen

- Every terminal pane now runs its own Ghostty emulator instance. On the shared instance, a pane opened after another one closed (for example a session started after deleting one) was handed the closed pane's memory uncleared, so it showed the old session's output with the new one drawn over it. An instance costs about 1.2 MB and is released with its pane.
