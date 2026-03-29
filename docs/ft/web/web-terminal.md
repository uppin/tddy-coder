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

- **Fullscreen**: Fills 100% of the viewport (width and height). **Connection chrome** overlays sit above the terminal canvas (high `z-index`, pointer events on controls only).
- **Auto-focus**: Keyboard focus is set on the terminal when ready. User can type immediately. (On mobile, auto-focus is disabled; see Mobile UX.)
- **Adaptive size**: FitAddon auto-sizes the terminal to its container. Resize events are sent to the virtual TUI via `\x1b]resize;{cols};{rows}\x07`.
- **Touch/mouse mode**: When `--mouse` is set on tddy-coder, the TUI sends EnableMouseCapture. GhosttyTerminal encodes SGR mouse sequences `\x1b[<Pb;Px;PyM/m` (press/release) and forwards them via onData. Click-to-select and scroll work. Touch events (touchstart/touchend) are forwarded for tap-to-click on mobile.

### Connection chrome (LiveKit overlay)

When **`GhosttyTerminalLiveKit`** is mounted with **`connectionOverlay`**, the shell includes:

- **Build ID**: Shown top-left when provided (`data-testid="build-id"`).
- **Status dot**: Fixed top-right (`data-testid="connection-status-dot"`). Attribute **`data-connection-status`** reads **`connecting`**, **`connected`**, or **`error`** for the LiveKit / token phase. While **`connecting`**, the dot uses a pulse animation; steady colors distinguish **`connected`** and **`error`**. Users who prefer reduced motion receive a non-animated connecting state via **`prefers-reduced-motion`**.
- **LiveKit status strip**: With the overlay enabled, the plain **`livekit-status`** row does not occupy layout during **`connecting`** or **`connected`**; the dot carries those phases. Token, room, and stream failures surface through **`data-testid="livekit-error"`** and related error UI.
- **Fullscreen**: A dedicated control (`data-testid="terminal-fullscreen-button"`, top-right beside the dot) enters or exits document fullscreen on the connected terminal subtree. The implementation uses the standard Fullscreen API with vendor-prefixed fallbacks (`packages/tddy-web/src/lib/browserFullscreen.ts`). The parent passes **`fullscreenTargetRef`** to select the element; when absent, chrome supplies an internal fullscreen target wrapper (`data-testid="connection-chrome-fullscreen-fallback-target"`). **`fullscreenchange`** and **`webkitfullscreenchange`** on **`document`** keep the control label in sync with the active element.
- **Menu**: Activating the dot opens a menu with **Disconnect** (`data-testid="connection-menu-disconnect"`) and **Terminate** (`data-testid="connection-menu-terminate"`) when the host passes **`onTerminate`** (daemon flows with session context). The standalone GitHub connect flow omits **Terminate** when no session-backed handler exists. **Terminate** runs a native **`window.confirm`** dialog; **`onTerminate`** runs only after the user confirms. The menu closes on **Escape** or an outside pointer press.
- **Stop**: A **Stop** control (`data-testid="terminal-stop-button"`, bottom-right, touch-friendly minimum size) sends byte **0x03** through the same **`enqueueTerminalInput`** path as keyboard **Ctrl+C**.

**ConnectedTerminal** wrappers (**App** after connect and **ConnectionScreen** after session connect) render the fullscreen **`connected-terminal-container`** with this chrome during JWT acquisition so the status dot reflects the loading phase while **`livekit-status`** text stays suppressed for normal overlay states.

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
- **Projects** as collapsible sections (`<details>`): each shows name, git URL, `main_repo_path`, then **Host** (target daemon instance from `ListEligibleDaemons`), **Tool** (options from `ListTools`, reflecting daemon `allowed_tools`), **Backend** (options from `ListAgents`, reflecting daemon `allowed_agents`; each option’s value is the agent **`id`** sent on **`StartSession.agent`**; the selected backend is the first list entry unless a prior choice for that project still appears in the list), **Workflow recipe** (`tdd` or `bugfix` on `StartSession.recipe`), and **Debug logging** (browser terminal only)—all **per session**, not stored on the project—then **Start New Session** (`StartSession` with `project_id`, optional `daemon_instance_id`, and `recipe`), and a table of sessions for that `project_id`. Session tables include a **Host** column (`daemon_instance_id` from `ListSessions`). Connect/Resume in that section uses that project’s debug setting.
- After authentication, the client loads **Tool** and **Backend** options together (`ListTools` and `ListAgents`); a failure in either RPC clears both lists and surfaces an error in the shared connection error area.
- **Other sessions**: Connect/Resume uses a separate **debug** checkbox for that list (sessions not tied to a listed project).
- Sessions whose `project_id` is not in the listed projects appear under **Other sessions**.

### Session table ordering

Each project’s session table and the **Other sessions** table list rows in this order:

1. **Active sessions** (`isActive` true) appear before inactive rows.
2. Within the active group and within the inactive group, rows follow **`createdAt`** descending (newer timestamps first), using ISO-8601 strings parsed with the browser **`Date`** implementation.
3. When two rows share the same comparable time, or when **`createdAt`** does not parse to a finite time, order follows **`sessionId`** lexicographically (deterministic tie-break).

The client applies **`sortSessionsForDisplay`** (`packages/tddy-web/src/utils/sessionSort.ts`) to the session array already held in React state after **`ListSessions`**—no additional RPC for ordering. In Vite development builds, optional **`console.debug`** / **`console.info`** traces run when **`import.meta.env.DEV`** is true.

### Session workflow status (TUI parity)

Project session tables and the **Other sessions** table include five additional columns—**Goal**, **Workflow**, **Elapsed**, **Agent**, and **Model**—alongside ID, Date, Status, Repo, PID, and Actions. The UI renders the string fields on each **`SessionEntry`** returned by **`ListSessions`**: **`workflow_goal`**, **`workflow_state`**, **`elapsed_display`**, **`agent`**, and **`model`**. Empty or whitespace-only values display an em dash (`—`).

The daemon fills these fields from each session directory’s **`.session.yaml`** (session identity) and, when present, **`changeset.yaml`**: the workflow goal is the matching session row’s **tag**; workflow state is **`state.current`**; the agent is the row’s **agent**; the model label is **`models[tag]`** when defined. **Elapsed** is a compact duration string produced with the same rules as the TUI status bar formatter (**`tddy_core::format_elapsed_compact`**), computed from persisted **`state.history`** timestamps (last transition whose state matches **`state.current`**, or **`state.updated_at`**). The browser shows a horizontally scrollable table when the viewport is narrower than the full column set.

While the session list includes at least one row with **`isActive`**, the client requests **`ListSessions`** every **2** seconds; when every row is inactive, the interval is **5** seconds. **`ListProjects`** continues to refresh every **5** seconds. Authentication and user mapping for **`ListSessions`** match other RPCs (GitHub token → mapped OS user → sessions base).

#### TUI vs web elapsed (QA)

- **TUI (`format_status_bar`)**: Elapsed is **`goal_start_time.elapsed()`** — an in-memory **`Instant`** from when the current workflow step started in the running **`tddy-coder`** process.
- **Web / daemon (`ListSessions` enrichment)**: Elapsed is **`format_elapsed_compact(now - step_start)`** where **`step_start`** is parsed from **`changeset.yaml`**: the **`at`** timestamp of the **last** **`state.history`** entry whose **`state`** matches **`state.current`**, or else **`state.updated_at`**. The web shows **persisted** wall-clock duration since the last recorded transition, not the in-process **`Instant`**.
- **Comparison**: When the workflow has **persisted** the latest state to **`changeset.yaml`**, web and TUI **should align** on goal, state, agent, model, and a **similar** elapsed string (same formatting rules in **`tddy_core::format_elapsed_compact`** and TUI **`format_elapsed`**). If the live process has **not yet written** **`changeset.yaml`**, the web may show an **older** elapsed or placeholders until the next **`ListSessions`** poll picks up new disk state.

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

### Fullscreen terminal session chrome

The fullscreen **GhosttyTerminalLiveKit** view opened from **Connect / Resume** uses the **connection chrome** described under [Connection chrome (LiveKit overlay)](#connection-chrome-livekit-overlay). **Terminate** in the dot menu, after confirmation, calls **`SignalSession`** with SIGTERM when the UI holds an active **session id** (same semantics as **Terminate (SIGTERM)** in the per-session **Signal** dropdown).

### Eligible daemons and host selection

- **`ListEligibleDaemons`**: After sign-in, **ConnectionScreen** loads eligible daemon entries (`instance_id`, `label`, `is_local`) alongside tools and projects. The daemon implementation lists instances from **`EligibleDaemonSource`** (currently the local daemon; LiveKit common-room peer discovery is deferred).
- **Host dropdown**: Per project, the selected host is sent as **`daemon_instance_id`** on **`StartSession`**. Empty or matching the local instance keeps the existing local spawn path. Selecting a non-local instance is rejected by the daemon until cross-daemon spawn routing exists.
- **Session host column**: **`ListSessions`** returns **`daemon_instance_id`** per row; the UI shows it in project and **Other sessions** tables.

## See also (development)

- [LiveKit and gRPC terminal RPC E2E](../../dev/guides/livekit-terminal-rpc-e2e.md) — `tddy-e2e` tests, VirtualTui vs LiveKit bidi behavior, assertion patterns.

## Future Scope

- LiveKit-based **peer daemon discovery** and **cross-daemon `StartSession` routing** (gateway delegates spawn to a peer over the common room control plane)
- Multi-session support
- Authentication and access control
- Session persistence and reconnection
