# 2026-10-25 — The jail reports its terminal size, in order with its output
**Type:** Feature

The PTY threads report the applied size after `openpty` and after each accepted resize as a
`SessionFrame.terminal_resized`, queued in the terminal backlog between the outputs around it. The
host relay delivers output and size changes as `HostTerminalEvent`s to a consumer that opts in with
`HostRelayConfig::with_terminal_events`; byte-only consumers are unchanged (#762, `#term-snapshot` 3/4).
