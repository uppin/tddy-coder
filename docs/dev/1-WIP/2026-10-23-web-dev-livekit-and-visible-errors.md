# 2026-10-23 — `./web-dev` owns its LiveKit and ports; the web UI says why instead of doing nothing

**Type:** Bug fix + Feature · **Status:** 🚧 WIP (draft PR; validation and wrap not run)

## Trigger

Creating a project in `./web-dev` showed neither an error nor a project. The daemon log had no
`CreateProject` at all, only `LiveKit connect failed: … validate request timed out` every ~34s. The
`.env` pointed at a LiveKit server that was down. With the common room's join failed, no RPC client
reached the host. `ProjectsAppPage` returned early on a `null` client, and every call ended in
`.catch(() => {})`.

Reproducing the follow-ups exposed three pre-existing macOS-only bugs in `./web-dev`:
- `/bin/bash` 3.2 has no `wait -n`, so the launcher fell straight into cleanup after starting Vite.
- Cleanup's `kill -- -<pid>` targeted process groups that did not exist, because job control was
  off.
- macOS `fuser` has no `-k`.

Together they left a daemon from an earlier run holding `:8899` with the dead LiveKit config, and it
answered the new run's readiness check.

## Scope

### `./web-dev` launcher
- [x] `scripts/dev-livekit.sh` — LiveKit in Docker by default: the pinned image, `--dev`, an
      ephemeral port block published 1:1 (ICE needs matching ports), and a new block if one is
      taken. An external server is reused only when `.env` sets all of
      `LIVEKIT_URL`/`LIVEKIT_API_KEY`/`LIVEKIT_API_SECRET`. A partial set is an error, and so is an
      unreachable server: there is no silent fall back to Docker.
- [x] `scripts/dev-process.sh` — `dev_wait_any` (replaces `wait -n`), `dev_require_free_port`
      (report, never kill), `dev_resolve_ports` (ephemeral unless pinned by `DAEMON_PORT`,
      `VITE_PORT` or a port in `VITE_URL`), and the run record (`tmp/web-dev.run`).
- [x] `web-dev`:
  - children run with `set -m`, and `on_exit` stops them on every exit (EXIT, INT, TERM, HUP);
  - no `fuser`;
  - `vite --strictPort`;
  - the daemon port is written into its temp config;
  - leftovers recorded by an earlier run are reaped, but only pids whose start time still matches;
  - the run refuses to start while the recorded run is alive.
- [x] Ports stable per checkout: derived from the checkout path within the ephemeral range, falling
      back to a random free port with a notice. That keeps the browser origin, and with it
      `localStorage` (session token, host-key pins), across restarts. The LiveKit block avoids the
      daemon and Vite ports. KILL is sent only to a pid whose start time still matches. The readiness
      wait fails as soon as the daemon exits.
- [~] Deferred (developer-approved): media over a LAN `VITE_URL` —
      [2026-10-23-web-dev-livekit-lan-media.md](../todo/2026-10-23-web-dev-livekit-lan-media.md).

### Web UI — visible errors (Projects + sessions)
- [x] Shared: `src/rpc/rpcFailure.ts` (`rpcFailureText`, `hostNotConnected`),
      `src/components/ui/inline-error.tsx`, `src/hooks/useActionFailure.ts`.
- [x] Projects:
  - create / add-to-host / default branch / accounts / branches show the daemon's reason where
    the action was taken;
  - a failed list load is never shown as an empty list;
  - the create form keeps its input on failure and shows "Creating…" while in flight.
- [x] Session inspector tabs: VNC, screen sharing, files, worktree (+ stats), tools, terminals.
- [x] Sessions drawer and runtime:
  - list load, resume / delete / terminate with no host, auto-attach;
  - bulk delete shows one line per failed row;
  - child runtime connect;
  - terminal claim;
  - undelivered terminal input shows a single indicator.
- [x] Create-session pickers (projects, tools, stack parents, branches) and session streams
      (notifications, usage, activity).
- [x] Root-cause fix, also present on master: the React Compiler (on in dev and prod, off in
      Cypress) dropped the unread `revision` dependency of `useHostConnector` and the unread
      `listedCount` of `SelectedDaemonScope`'s `isReady`. Every `useDaemonClient` therefore stayed
      `null` once the common room joined after the first render. Both opt out with `"use no memo"`.
      Verified in Chromium against a live `./web-dev`.
- [x] Bulk delete has no fallback: an unreachable owner fails that row with "Not connected to <host>".
- [x] Attach / delete / signal errors carry no transport `[code]` prefix.
- [x] Splits under the 500-line budget: `SessionRuntime.tsx` 694→440, `SessionsDrawerScreen.tsx`
      582→483, `CreateSessionPane.tsx` 516→484.
- [x] Shared helpers: `hostNotConnectedText`, `actionFailed`, `useHostLabel`, `useKeyedActionFailure`.
- [~] Deferred (developer-approved):
  - session status streams reconnect —
    [2026-10-23-session-status-streams-do-not-reconnect.md](../todo/2026-10-23-session-status-streams-do-not-reconnect.md)
  - gating Accounts / Worktrees / VMs —
    [2026-10-23-host-gate-other-screens.md](../todo/2026-10-23-host-gate-other-screens.md)
- [x] Layout gate: `AppShell requiresHost` → `HostConnectionGate`.
  - Blocks the body, with the host and reason, when no viable RPC client reaches the selected host:
    no host selected, no connection, or a LiveKit connection whose room failed.
  - The header stays usable.
  - Opted in: Projects and Sessions.

## Open decisions / known gaps
- ⚠️ Cypress runs with the React Compiler off, so the compiler-only bug class above is invisible to
  every component test. Only a real-browser run caught it.
- `useSessionActivity` has no production consumer, which was already true before this change. Its
  error is covered through a harness.
- "Unknown service: accounts.AccountsService" shows on Projects when the daemon has no credential
  vault (`dev.daemon.yaml`). This is accurate: `AccountsService` is mounted only with vaults. Wording
  is open.
- ⚠️ Not tested: connect-after-create (unreachable in practice) and input failures on the LiveKit
  terminal path (`roomTerminalFeed.ts` gets no per-send answer).

## Verification (scoped, local; CI not run)
- `tddy-e2e`:
  - `dev_livekit_script` 19/19
  - `dev_process_script` 39/39
  - `web_dev_script` 6/6
  - lib `shell_code_lines` 1/1
  - clippy and fmt clean
- `tddy-web` unit (`test:unit`): 1249/1249.
- `tddy-web` Cypress component, scoped: 173 specs, 1073/1073. These are every spec that mounts a
  touched file, plus the five new specs.
- Real runs on macOS `/bin/bash` 3.2:
  - `./web-dev` served on ephemeral ports;
  - TERM left nothing behind;
  - after a `kill -9`, the next run stopped the dead run's daemon, Vite and container;
  - Ctrl+C runs `on_exit`, checked under a pty with bash 3.2 and 5.3.
- Chromium (Playwright) against a live `./web-dev`:
  - Projects loads, with `ListProjects` reaching the daemon over LiveKit;
  - before the compiler fix, the gate showed "mac is not in the common room".
