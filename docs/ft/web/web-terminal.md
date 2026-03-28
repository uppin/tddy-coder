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

- **Create project** (collapsible): name + git URL → `CreateProject` (clone or adopt existing path under `~/repos/<name>/` by default). Optional **path under home** overrides the clone destination (e.g. `Code/my-app`).
- **Projects** as collapsible sections (`<details>`): each shows name, git URL, `main_repo_path`, then **Tool** (from daemon `allowed_tools`), **Backend** (`agent` on `StartSession`), and **Debug logging** (browser terminal only)—all **per session**, not stored on the project—then **Start New Session** (`StartSession` with `project_id`), and a table of sessions for that `project_id`. Connect/Resume in that section uses that project’s debug setting.
- **Other sessions**: Connect/Resume uses a separate **debug** checkbox for that list (sessions not tied to a listed project).
- Sessions whose `project_id` is not in the listed projects appear under **Other sessions**.

### Inactive session deletion

- **Inactive rows** (`!isActive`): The actions column shows **Resume** and **Delete**. **Delete** opens a browser **confirm** dialog; on confirmation the client calls **`DeleteSession`** with the session id, reloads the session list on success, and shows RPC errors in the same error area as other connection actions.
- **Active rows**: **Connect** and **Signal** appear; **Delete** is absent.
- **Orphan table** follows the same inactive vs active rules as project session tables.

The daemon implements **`DeleteSession`** with the same GitHub user → OS user → sessions base resolution as **`ListSessions`**, removes only the target directory under that tree when the session is inactive (no live PID for the value stored in `.session.yaml`), and returns gRPC errors for missing sessions, active processes, or invalid ids. See [daemon changelog](../daemon/changelog.md) and [connection-service.md](../../../packages/tddy-daemon/docs/connection-service.md).

See [daemon project concept](../daemon/project-concept.md).

### Shared LiveKit room (`livekit.common_room`)

When the daemon sets **`livekit.common_room`** in YAML, that name is exposed to the web client as **`common_room`** in **`GET /api/config`** (with **`livekit_url`**). After GitHub sign-in, the browser joins that room with identity **`web-{githubLogin}`** and shows a **Connected participants** table on the session list screen (identity, role, joined time, metadata), updated live via LiveKit participant events. The fullscreen terminal opened by **Connect / Resume** is unchanged; the presence connection remains active in the background while the terminal is open.

If **`common_room`** is unset or blank, that panel is not shown and no extra LiveKit connection is made for presence.

Spawned **`tddy-*`** sessions use the same configured room for **`--livekit-room`** when **`common_room`** is set; each process still uses a distinct **`daemon-{session_id}`** LiveKit identity for terminal RPC. If **`common_room`** is unset, the room name is **`daemon-{session_id}`** per session. See [daemon changelog](../daemon/changelog.md).

## See also (development)

- [LiveKit and gRPC terminal RPC E2E](../../dev/guides/livekit-terminal-rpc-e2e.md) — `tddy-e2e` tests, VirtualTui vs LiveKit bidi behavior, assertion patterns.

## Future Scope

- Multi-session support
- Authentication and access control
- Session persistence and reconnection
