# 2026-07-25 — enqueued-input-overlay: terminal input-offset ACK. `PtyHandle::send_input(data, input_offset)` records the applied offset on the shared `tddy_task::TaskChannel` (not the per-RPC-rebuilt handle) and publishes it; `send_terminal_input` threads `req.input_offset`; `stream_terminal_output` subscribes to the channel's ack `watch` and interleaves `SessionTerminalOutput{data:[], acked_input_offset}` frames (initial offset emitted up front). Acceptance: `tests/terminal_input_ack_acceptance.rs`. See [connection-service.md § Input-offset acknowledgement](../connection-service.md#input-offset-acknowledgement) and feature [enqueued-input-overlay.md](../../../../docs/ft/web/enqueued-input-overlay.md). (tddy-daemon)

**Type:** Feature


