# 2026-10-23 — `./web-dev` owns its LiveKit and ports; the web UI reaches hosts under the React Compiler and says why actions fail

**Type:** Fix

Creating a project in `./web-dev` did nothing: no error appeared, and no project either. There were
three causes.

1. **The React Compiler, also on `master`.** It runs in dev and production builds and is off in
   Cypress. It dropped the unread `revision` dependency of `useHostConnector` and the unread
   `listedCount` of `SelectedDaemonScope`'s `isReady`, so a page whose common room joined after the
   first render had no RPC client for any host. Both functions now opt out with `"use no memo"`.
   This was verified in Chromium against a live `./web-dev`.
2. **Every failure was hidden.** Projects and the session surfaces swallowed a `null` client and
   every RPC failure. They now show the daemon's reason where the action happened, a layout gate
   blocks host-scoped screens while nothing reaches the host, and bulk delete no longer falls back
   to the selected daemon.
3. **`./web-dev` on macOS `/bin/bash` 3.2.** It has no `wait -n`, and `fuser` there has no `-k`.
   The launcher exited at once, its daemon and Vite were left running, and the next run's
   readiness check was answered by that stale daemon.

What `./web-dev` does:
- It starts a LiveKit container unless `.env` names a complete, reachable server.
- Ports are stable per checkout, from the ephemeral range.
- A pinned port that is already held is an error; the launcher never kills anything.
- Children run in their own process groups and are stopped on every exit.
- A start-time-checked run record (`tmp/web-dev.run`) cleans up after a `kill -9`.

Permanent docs:
- [local-web-dev.md](../../ft/web/local-web-dev.md)
- [operator-visible-failures.md](../../../packages/tddy-web/docs/operator-visible-failures.md)
- [host-connections.md](../../../packages/tddy-web/docs/host-connections.md) (§ Under the React
  Compiler, § The host connection gate)
- [projects-screen.md § Failures](../../../packages/tddy-web/docs/projects-screen.md#failures)

Package entries:
- [tddy-web](../../../packages/tddy-web/docs/changesets/2026-10-23-visible-failures-and-host-gate.md)
- [tddy-e2e](../../../packages/tddy-e2e/docs/changesets/2026-10-23-dev-launcher-script-tests.md)

Code issues closed in `tddy-web`, measured with `wc -l` at wrap:
- `oversized-file-create-session-pane`: 484 lines. The agent selection moved to
  `useCreateSessionAgentSelection.ts`, and the tools and stack-parent errors moved into their field
  components.
- `oversized-file-session-runtime`: 440 lines, from 646 at detection and 694 mid-change. It is now
  `SessionChildRuntime.tsx` 131, `sessionRuntimeProps.ts` 58, `useSessionPaneFullscreen.ts` 76 and
  `useAgentTerminalFocus.ts` 80.

`SessionsDrawerScreen.tsx` went from 582 lines mid-change to 483. `useSessionsDrawerPaneActions.ts`
(134) and `SessionsDrawerNotices.tsx` (70) were split out of it.

Deferred to the backlog, all developer-approved:
- `2026-10-23-web-dev-livekit-lan-media`
- `2026-10-23-session-status-streams-do-not-reconnect`
- `2026-10-23-host-gate-other-screens`
- `2026-10-23-cypress-runs-without-the-react-compiler`, which records the finding this change exposed

Verification was local and scoped; CI did not run:
- tddy-e2e script suites: 19/19, 39/39 and 6/6;
- tddy-web unit tests: 1249/1249;
- Cypress, every spec mounting a touched file: 173 specs, 1073/1073;
- real `./web-dev` runs under bash 3.2, covering TERM, `kill -9` followed by a re-run, and Ctrl+C.
