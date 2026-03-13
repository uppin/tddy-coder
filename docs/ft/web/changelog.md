# Web Changelog

Release note history for the Web product area.

## 2026-03-13 — Ghostty Terminal Integration via LiveKit

- **GhosttyTerminal**: React component wrapping ghostty-web for ANSI terminal rendering. Standalone (no LiveKit dependency); used by Storybook and LiveKit-connected story.
- **GhosttyTerminalLiveKit**: Storybook story that connects to tddy-demo via LiveKit, streams TerminalOutput to GhosttyTerminal, pipes onData back as TerminalInput.
- **TerminalService**: New RPC in tddy-livekit (StreamTerminalIO) — bidirectional streaming of terminal bytes over LiveKit data channels.
- **tddy-demo LiveKit args**: `--livekit-url`, `--livekit-token`, `--livekit-room`, `--livekit-identity` wire terminal byte capture to LiveKit participant.
- **E2E test**: Cypress startTerminalServer/stopTerminalServer tasks; asserts streamed bytes and terminal buffer content through full stack.
- **Supersedes**: WebSocket-based web-terminal approach; streaming tddy-coder TUI is now implemented via LiveKit.
