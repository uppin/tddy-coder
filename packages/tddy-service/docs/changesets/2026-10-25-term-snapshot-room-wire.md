# 2026-10-25 — The room wire's terminal carries snapshots and offsets
**Type:** Bug Fix

`terminal.proto` gains `TerminalInput.request_snapshot` and `TerminalOutput.{snapshot, start_offset,
end_offset}`. `TerminalServiceVirtualTui` (rpc and tonic) stamps each connection's output with
contiguous offsets, sends its first frame and every full repaint with a leading RIS flagged as a
snapshot, and forwards a later `request_snapshot` to the VirtualTui as `VIRTUAL_TUI_REPAINT_REQUEST`
after the message's bytes (#763, `#term-snapshot` 4/4).
