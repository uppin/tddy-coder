# Web Terminal

## Summary

A web application that displays an interactive terminal in the browser using the ghostty-web terminal emulator. **tddy-demo TUI streaming over LiveKit** is implemented: GhosttyTerminal component in tddy-web receives ANSI bytes via TerminalService RPC, with Cypress E2E validation. A standalone generic terminal (user's default shell over WebSocket) remains available via the Ghostty-web demo.

## Recommended Setup

Use the official Ghostty-web demo:

```bash
./serve_term.sh
# or: npx @ghostty-web/demo@next
```

Serves at http://localhost:8080 with WebSocket PTY at `/ws`. Works best on Linux and macOS.

## Background

tddy-coder currently operates as a CLI/TUI application. To enable remote observation and collaboration, a web-based terminal viewer is needed. A previously implemented Dioxus fullstack solution was archived; its architecture is documented in [docs/kb/dioxus-web-terminal.md](../../kb/dioxus-web-terminal.md).

## Architecture (Reference)

- **Terminal emulator**: ghostty-web npm package (WASM-compiled Ghostty VT100 parser, xterm.js-compatible API)
- **PTY**: Server-side PTY spawning the user's default `$SHELL`
- **Communication**: WebSocket between browser (ghostty-web Terminal) and server (PTY process)

## Connected Terminal UX

When the terminal connects and renders, it supports:

- **Fullscreen**: Fills 100% of the viewport (width and height). Overlay buttons: Disconnect and Ctrl+C.
- **Auto-focus**: Keyboard focus is set on the terminal when ready. User can type immediately. (On mobile, auto-focus is disabled; see Mobile UX.)
- **Adaptive size**: FitAddon auto-sizes the terminal to its container. Resize events are sent to the virtual TUI via `\x1b]resize;{cols};{rows}\x07`.
- **Touch/mouse mode**: When `--mouse` is set on tddy-coder, the TUI sends EnableMouseCapture. GhosttyTerminal encodes SGR mouse sequences `\x1b[<Pb;Px;PyM/m` (press/release) and forwards them via onData. Click-to-select and scroll work. Touch events (touchstart/touchend) are forwarded for tap-to-click on mobile.

### Mobile UX

On touch-capable devices or narrow viewports (width &lt; 768px):

- **Keyboard-aware resize**: The terminal container uses the Visual Viewport API. When the virtual keyboard opens, the container shrinks to fit the visible area above the keyboard; when it closes, the terminal fills the screen again.
- **Manual keyboard button**: A floating "Keyboard" button appears at the bottom center. Tapping it focuses the terminal (opens the virtual keyboard). The button hides while the keyboard is open and reappears when it closes.
- **Focus prevention**: Tapping the terminal does not open the keyboard. The terminal uses `preventFocusOnTap` (event prevention + readonly textarea) so the keyboard opens only when the user taps the Keyboard button.
- **Touch forwarding**: Tap-to-click works for TUI menus and interactive elements. Capture-phase touch handlers send SGR mouse sequences before focus prevention, so interactive TUIs (vim, htop) receive correct mouse events.
- **Build ID**: A build timestamp is shown in the top-left when connected for cache verification on mobile.

## Daemon mode: Connection screen (project-centric)

When `tddy-daemon` serves the web bundle (`daemon_mode: true`), authenticated users see **ConnectionScreen** (not the manual LiveKit URL form):

- **Tool** dropdown from daemon `allowed_tools`.
- **Create project** (collapsible): name + git URL → `CreateProject` (clone or adopt existing path under `~/repos/` by default).
- **Projects** as collapsible sections (`<details>`): each shows name, git URL, `main_repo_path`, **Start New Session** (`StartSession` with `project_id`), and a table of sessions for that `project_id`.
- Sessions whose `project_id` is not in the listed projects appear under **Other sessions**.

See [daemon project concept](../daemon/project-concept.md).

## Future Scope

- Multi-session support
- Authentication and access control
- Session persistence and reconnection
