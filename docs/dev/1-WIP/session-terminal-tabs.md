# Changeset: Session terminal tabs (multiple terminals per session)

**PRD**: `docs/ft/web/session-terminal-tabs.md`
**Branch**: `feat/sesson-terminal`

## Summary

Add a **terminal tab bar** to a session's detail pane so the user can switch between the coding
agent and one or more interactive **bash** terminals; multiple terminals per session, over **both**
transports. The Agent tab (reserved `"main"`) is fixed/non-closable; `+` opens bash terminals, each
closable (`✕` → `StopTerminalSession`). Backgrounded terminals of the focused session stay mounted
and keep streaming. The daemon already supports this for local (`connected-grpc`) sessions; this
changeset wires the web UI and adds the coder-side PTY path for remote (`connected-livekit`)
sessions. PTY plumbing shared by daemon + coder moves into a new `tddy-pty` crate.

## Checklist

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Write acceptance tests (Cypress component) — 5/5 passing
- [x] Write unit/integration tests (Rust) — 6/6 passing
- [x] Layer 0: extract `tddy-pty` crate; refactor `tddy-daemon` onto it
- [x] Layer 1: coder terminal manager + participant terminal RPCs (start/stop/list/stream/send)
- [x] Layer 2: web terminal tabs (`SessionTerminalTabs`, `useSessionTerminals`, `terminalId` threading)
- [x] Verify — combined build of `tddy-pty` + `tddy-daemon` + `tddy-coder` clean (4m15s); coder
  `session_participant` suite 21/21 (incl. the 6 terminal tests); Cypress spec 5/5 + regression
  guards green. Remaining: manual `./web-dev` both-transports smoke (LiveKit/coder path)

## Acceptance criteria (behaviour)

1. A connected session shows a terminal tab bar with an **Agent** tab that has no close control.
2. `+` calls `StartTerminalSession`; the returned `terminal_id` appears as a new bash tab, becomes
   active, and opens `StreamTerminalOutput` for that `terminal_id`.
3. A second `+` yields two bash tabs; switching tabs keeps every terminal of the session mounted
   (background terminals keep streaming — no unmount / re-stream on focus switch).
4. Closing a bash tab calls `StopTerminalSession(terminal_id)`, removes the tab, and returns focus
   to the Agent tab when the closed tab was active.
5. Keyboard input routes to the **active** tab's `terminal_id` (`SendTerminalInput`).

## Files to create

| File | Purpose |
|------|---------|
| `packages/tddy-pty/` (crate: `Cargo.toml`, `src/lib.rs`, `src/runtime.rs`, `src/registry.rs`) | Shared PTY spawn + registry: `PtySpawnSpec`, `PtyReady`, `PtyRuntime`, `open_pty_and_pump`, `PtyRegistry`/`PtyControl` (no OS-user impersonation) |
| `packages/tddy-coder/src/session_participant/terminal_manager.rs` | Coder-side `session_id → (terminal_id → PtyHandle)` manager over `tddy-pty` + the coder's `TaskRegistry`; spawns `$SHELL`/`/bin/bash` in the worktree |
| `packages/tddy-web/src/components/sessions/SessionTerminalTabs.tsx` | Tab strip: fixed Agent tab + closable bash tabs + `+` |
| `packages/tddy-web/src/components/sessions/useSessionTerminals.ts` | Hook: `ListTerminalSessions` on attach; `{ terminals, activeTerminalId, setActive, open(), close(id) }` |
| `packages/tddy-web/cypress/component/SessionTerminalTabsAcceptance.cy.tsx` | Acceptance tests (in-memory RPC backend) |
| `packages/tddy-web/cypress/support/pages/sessionTerminalTabsPage.ts` | Page object for the tab bar |

## Files to modify

| File | Change |
|------|--------|
| `Cargo.toml` (workspace) | Add `packages/tddy-pty` member |
| `packages/tddy-daemon/src/pty_runtime.rs`, `pty_registry.rs` | Refactor onto `tddy-pty`; keep daemon-only OS-user impersonation (pre-wrap argv/env, call shared spawner) |
| `packages/tddy-daemon/Cargo.toml` | Depend on `tddy-pty` |
| `packages/tddy-coder/Cargo.toml` | Depend on `tddy-pty` |
| `packages/tddy-coder/src/session_participant/mod.rs`, `connection_service_participant.rs` | Serve `StartTerminalSession`/`StopTerminalSession`/`ListTerminalSessions` (unary), `SendTerminalInput` (unary, by `terminal_id`), `StreamTerminalOutput` (`RpcResult::ServerStream`, by `terminal_id`); reject `Stop("main")` |
| `packages/tddy-coder/src/run.rs` | Construct + wire the terminal manager into the session `ConnectionService` participant |
| `packages/tddy-web/src/components/sessions/SessionRuntime.tsx` | Render the tab bar; mount one terminal per `terminal_id` (active visible, rest hidden, all mounted); thread `terminalId` |
| `packages/tddy-web/src/components/sessions/GrpcSessionTerminal.tsx` | Accept `terminalId`; pass to `streamTerminalOutput` / `sendTerminalInput` |
| `packages/tddy-web/src/components/sessions/SessionMainPane.tsx` | Prop passthrough |
| `packages/tddy-web/cypress/support/testIds.ts` | Add tab test-ids (`sessions-terminal-tab*`) + dynamic helpers |
| `packages/tddy-web/cypress/support/rpc/connectionServiceBackend.ts` | Implement `startTerminalSession`/`stopTerminalSession`/`listTerminalSessions`/`sendTerminalInput`/`streamTerminalOutput`; record calls; dynamic terminal list |

## Design decisions

### One shared PTY crate, impersonation stays in the daemon
`PtyRuntime` is transport/host-agnostic except OS-user impersonation (setpriv, passwd lookup,
HOME/PATH). The daemon runs as root and impersonates; the **coder already runs as the target user**,
so it spawns with `os_user: None`. Extracting the core into `tddy-pty` (dep: `tddy-task` +
`portable_pty`) lets both reuse the I/O threading, capture, and resize without the coder taking a
`tddy-daemon` dependency. No new external dependency.

### Agent tab is transport-specific; bash tabs are uniform
The Agent (`"main"`) tab keeps its existing renderer per transport (gRPC bidi terminal, or LiveKit
VirtualTui). Bash tabs are uniform: `GrpcSessionTerminal` with a `terminalId`, over the daemon
client (`connected-grpc`) or the session-scoped LiveKit client (`connected-livekit`). `"main"` is
never a PTY on the coder; `StopTerminalSession("main")` is rejected.

### Control lease stays per-session
All of a session's terminals share the one existing per-session control lease; no per-terminal mutex.

## Validation Results

- **validate-changes**: combined build clean; 0 critical / 0 warning; 3 info (justified
  `exhaustive-deps` disable in `SessionRuntime.tsx`; broadcast `Lagged` drops output under extreme
  load, mirroring the daemon; Agent/"main" over `ConnectionService` returns `not_found` on coder
  sessions by design — Agent uses the VirtualTui path). Error paths log via the repo's debug logger
  (`dTerm`/`log::`), no swallowing, no secrets, no test-only branches, no stdout in TUI paths.

- **validate-tests**: fluent-compliant (Given/When/Then, page objects, `mountWithRpc` +
  `anInMemoryRpcBackend`, semantic fixtures, one behavior per test). No always-pass, `#[ignore]`,
  commented-out, or credential-hardcoding issues. Fixed one violation: moved a raw
  `[data-testid='ghostty-terminal']` selector out of the acceptance spec into a
  `sessionTerminalTabsPage.paneTerminal(id)` helper.

- **validate-prod-ready**: 0 blockers, 0 warnings. The diff introduces no new
  TODO/FIXME/`println!`/`eprintln!`/`console.log` and no mock/fake in production paths. INFO: the
  headless coder path defaults its terminal worktree to cwd (`unwrap_or_else(… ".")`) — benign CLI
  default, not error-masking.

- **analyze-clean-code**: score **B** (no must-refactor). Functions small (≤~55 lines), nesting ≤3,
  params ≤3, clear naming. Applied one improvement: named the `StreamTerminalOutput` bridge channel
  capacity (`TERMINAL_OUTPUT_CHANNEL_CAPACITY`). The `StreamTerminalOutput` match arm is long but
  kept inline to match the pre-existing `handle_rpc` arm style.

## Notes / open items

- Sandboxed claude-cli sessions: a bash terminal must spawn inside the same jail — confirm the
  daemon `start_terminal` path already runs in-sandbox; if not, flag with a TODO and scope to the
  non-sandboxed context first.
