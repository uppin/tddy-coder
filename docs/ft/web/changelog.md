# Web Changelog

Release note history for the Web product area.

## 2026-03-17 — Terminal UX: Fullscreen, Auto-Focus, Adaptive Size, Touch/Mouse

- **Fullscreen**: ConnectedTerminal fills 100% viewport. Overlay: Disconnect and Ctrl+C buttons.
- **Auto-focus**: Keyboard focus is set on the terminal when ready.
- **Adaptive size**: FitAddon auto-sizes to container. Resize escape sequence `\x1b]resize;{cols};{rows}\x07` flows to virtual TUI.
- **Touch/mouse**: `--mouse` flag on tddy-coder enables mouse capture. GhosttyTerminal encodes SGR mouse sequences and forwards via onData. Click-to-select and scroll work.

## 2026-03-14 — Token Fetch via Connect-RPC

- **Token form**: Identity, url, room. Connect-RPC client fetches tokens from server (GenerateToken, RefreshToken).
- **getToken prop**: GhosttyTerminalLiveKit accepts getToken for token refresh 1 minute before expiry.
- **Backward compat**: token prop still works; Storybook and e2e pass pre-generated tokens via URL params.

## 2026-03-13 — Ghostty Terminal Integration via LiveKit

- **GhosttyTerminal**: React component wrapping ghostty-web for ANSI terminal rendering. Standalone (no LiveKit dependency); used by Storybook and LiveKit-connected story.
- **GhosttyTerminalLiveKit**: Storybook story that connects to tddy-demo via LiveKit, streams TerminalOutput to GhosttyTerminal, pipes onData back as TerminalInput.
- **TerminalService**: New RPC in tddy-livekit (StreamTerminalIO) — bidirectional streaming of terminal bytes over LiveKit data channels.
- **tddy-demo LiveKit args**: `--livekit-url`, `--livekit-token`, `--livekit-room`, `--livekit-identity` wire terminal byte capture to LiveKit participant.
- **E2E test**: Cypress startTerminalServer/stopTerminalServer tasks; asserts streamed bytes and terminal buffer content through full stack.
- **Supersedes**: WebSocket-based web-terminal approach; streaming tddy-coder TUI is now implemented via LiveKit.
