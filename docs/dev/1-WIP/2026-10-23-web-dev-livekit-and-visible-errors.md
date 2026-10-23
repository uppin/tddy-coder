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
- [ ] ⚠️ TODO: media over a LAN `VITE_URL` — the container uses `--node-ip 127.0.0.1`.

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
- [x] Layout gate: `AppShell requiresHost` → `HostConnectionGate`.
  - Blocks the body, with the host and reason, when no viable RPC client reaches the selected host:
    no host selected, no connection, or a LiveKit connection whose room failed.
  - The header stays usable.
  - Opted in: Projects and Sessions.

## Open decisions / known gaps
- ⚠️ Bulk delete keeps a pre-existing fallback, `clientForHost(owner) ?? client`, so a row on an
  unreachable host is sent to the selected daemon instead. That breaks the no-fallbacks rule;
  removing it needs a decision.
- ⚠️ The activity, usage and notifications streams do not reconnect. Their error stays shown until
  a new subscription starts.
- ⚠️ Not tested: connect-after-create (unreachable in practice), opening a terminal with no
  client, and input failures on the LiveKit terminal path (`roomTerminalFeed.ts` gets no per-send
  answer).
- `useSessionActivity` has no production consumer, so its error is covered only by a test harness.
- Session attach errors still show the transport's `[code]` prefix (`useSessionAttachment.ts`).
- Accounts / Worktrees / VMs are not gated yet; each needs one prop.
- ⚠️ Not verified: Ctrl+C typed in a real terminal. TERM, HUP and `kill -9` + reap were run for
  real.

## Verification (scoped, local; CI not run)
- `tddy-e2e`:
  - `dev_livekit_script` 13/13
  - `dev_process_script` 19/19
  - `web_dev_script` 6/6
- `tddy-web` unit (`src/components/shell src/components/sessions src/rpc`): 656/656.
- `tddy-web` Cypress component, scoped:
  - 96 specs covering every spec that mounts a gated screen, plus the five new specs: 627/627.
  - The agents' earlier per-area runs also passed.
- Real runs on macOS `/bin/bash` 3.2:
  - ephemeral ports served (Vite 200, `/api/config` via proxy 200);
  - TERM left nothing behind;
  - `kill -9` of `web-dev`, then a second run, stopped the first run's daemon, Vite and container.
