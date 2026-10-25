# 2026-10-25 — Room-carried terminals open with a snapshot

A session carried over its own LiveKit room now shows the current screen on (re)connect, the same as
a host-served session. `terminal.TerminalService/StreamTerminalIO` opens with a snapshot frame at a
sequence point instead of a raw-output replay plus a forced redraw, stamps every frame with
contiguous offsets, and answers `request_snapshot` on any later message (or a lagging subscriber)
with a fresh snapshot. The coder's own TUI answers the same contract without ever resizing to
repaint. A client that reads only `data` still renders a snapshot, because it begins with a terminal
reset. See [Terminal sessions § Room-carried terminals](../terminal-sessions.md#room-carried-terminals-terminalterminalservicestreamterminalio).
