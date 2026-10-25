# 2026-10-25 — Room-carried session terminals reconnect to the current screen

A terminal for a session carried over its own LiveKit room asks for a snapshot on every open, so a
reconnect shows the current screen at once. Its frames carry offsets: one that does not continue the
screen is dropped and a fresh snapshot is asked for on the same stream, and its scrollback fill is
anchored like a host-served terminal's. See [Web terminal](../web-terminal.md).
