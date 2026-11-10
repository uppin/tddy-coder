# 2026-11-09 — The GitHub → OS-user mapping lives in `users.yaml`, owned by whoever runs the deployment

**Type:** Architecture

`#agent-tools` 31/32 ([#859](https://github.com/uppin/tddy-coder/pull/859)), branch
`feature/agent-tools/user-mapping-in-supervisor`. Builds on #834 (`#agent-tools` 26, the per-user
agent-tool sockets); file-overlap edge with #836 (`#agent-tools` 28). Successor:
`supervisor-agent-agnostic` (`#agent-tools` 32).

`users.yaml` (`- {github_user, os_user}` rows) is the one place a GitHub login is mapped to an OS user.
On a supervised host the root `tddy-supervisor` reads it (`users: {file, sockets_dir}` in
`supervisor.yaml`), derives **one** daemon socket per mapped OS user at
`<sockets_dir>/<os user>/daemon.sock` (`user.<os user>` in `LISTEN_FDNAMES`), hands the `serves_users`
service those listeners and the file's path in `TDDY_USERS_FILE`, and restarts it when the file
changes. An unsupervised daemon reads the `users.yaml` beside its config and binds
`<data dir>/run/<os user>/daemon.sock` itself. One socket carries both the host-session services and
the agent-tool roster, so the cap is **15** users (16 descriptors less fd 3), up from 7.

Removed: `spawn_policy.allowed_session_users`, `services[].host_sockets`,
`services[].agent_tool_sockets` (each a load error naming `users.yaml`), `daemon.yaml` `users:` (a load
error naming `users.yaml`), `SocketHandover::with_host_session_socket` / `with_agent_tool_socket`,
`HOST_SESSION_FD_NAME_PREFIX`, `AGENT_TOOLS_FD_NAME_PREFIX`, `resolve_host_session_fds`,
`agent_tool_socket_path_for_user`, `agent_tool_fds`, `adopt_from_environment`, `adopt_descriptors`,
`OwnerSocketKind::AGENT_TOOL`, `DaemonSessionHost::with_inherited_host_session_sockets`, and
`./install`'s `survey_agent_tool_sockets` / `declare_agent_tool_sockets`.

Decisions recorded at the wrap:

- **`UpdateConfig` writes no `users.yaml`.** Its wire message carries no mapping; it only stops
  writing `users:` into `daemon.yaml`. First-login enrolment is `users.yaml`'s one writer (user
  decision on the pass-2 report; the PRD's "enrolment and `UpdateConfig` write `users.yaml`"
  criterion was wrong and is corrected in the permanent docs).
- A missing `users.yaml` is an error everywhere, never an empty mapping. `./install` (`--systemd`,
  `--user`, `--desktop`) renders an empty one at `0644`; `./web-dev` renders one in a per-run
  `mktemp -d` directory beside its temp config; `./desktop-dev` renders the checkout-root `users.yaml`
  only for the checkout's own `dev.desktop.yaml`, never for a passed-in `TDDY_DAEMON_CONFIG`.
- `./install` migrates once, idempotently: `daemon.yaml` `users:` → `users.yaml` (never overwriting
  one that maps somebody), the three `supervisor.yaml` lists dropped.
- The natural three-way split (merged socket / `users.yaml` + handover + migration / watch + restart)
  was offered twice and declined; one node, complexity 5.

Behaviour fixed at validation:

- A `users.yaml` change stops each `serves_users` service with `SIGTERM`, then `SIGKILL`, each bounded
  by 10 s, and then restarts it. A service that survives both is reported as an error, not left
  silently unrestarted (its stop was requested, so a later exit would have read as "stopped on
  request").
- Changes are applied one at a time, in save order.
- Enrolment into a missing `users.yaml` is refused `ConfigNotWritable`, never read as an empty mapping.
- The watch refuses to start if `users.yaml` cannot be canonicalized.

Known risk, accepted: **a mapping change restarts the daemon**, dropping every relayed agent and its
in-memory state (open vaults, room connections, inference subscriptions); sessions resume from disk.
No live supervisor→daemon channel was added.

Backlog and code issues:

- `2026-11-01-install-script-over-length-budget` — **narrowed**, kept: `install` 1,700 → 1,600 lines
  (1,053 → 977 non-blank, non-comment); the agent-tool-socket survey is gone; the account/identity
  split stays open.
- `2026-11-09-dead-inherited-host-session-socket-adoption` — **added**: `inherited_host_sockets.rs`,
  `HostSessionSockets::adopt_inherited`, the `host-session.` adoption tests and
  `OwnerSocketKind::HOST_SESSION`'s stale setting have no production caller; deleting the module
  needs developer consent.
- `tddy-supervisor` `oversized-file-spawn-broker` — 977 → 984 (grown; not split, #836/#857/#865 also
  modify it). `oversized-file-config` — **added**, `config.rs` 463 → 507 (split deferred, #865 also
  modifies it). `oversized-file-supervisor` stays closed: `supervisor.rs` 388 → 422, the watch and
  restart living in `users_watch.rs` and `supervisor/users_change.rs`.
- `tddy-daemon-auth` `oversized-file-auth` — 579, unchanged (tests only).
- Referenced, unchanged: `2026-10-08-keyring-supervisor-host-socket-real-run` (the merged socket goes
  through the same unrun root handover), `2026-09-18-desktop-install-configures-no-identity`.

Test-fixture churn: about 25 suites in `tddy-daemon-livekit`, `tddy-host-service`, `tddy-daemon-auth`,
`tddy-model-registry`, `tddy-daemon`, `tddy-daemon-rpc`, `tddy-session-lifecycle` and
`tddy-worktree-service` write `users.yaml` beside their config or set `config.users` in memory. Four
`tddy-e2e` agent-tool-socket install tests were deleted with approval.

Docs: [tddy-supervisor](../../ft/supervisor/tddy-supervisor.md),
[systemd install](../../ft/daemon/systemd-install.md), [session auth](../../ft/daemon/session-auth.md),
[daemon settings](../../ft/daemon/daemon-settings.md),
[Tddy Desktop](../../ft/desktop/tddy-desktop-tauri.md),
[host sockets](../../../packages/tddy-supervisor/docs/host-sockets.md),
[daemon kernel](../../../packages/tddy-daemon-kernel/docs/daemon-kernel.md).
Package entries: tddy-supervisor, tddy-daemon-kernel, tddy-daemon, tddy-daemon-auth, tddy-agent-launch,
tddy-session-lifecycle, tddy-desktop, tddy-e2e (`docs/changesets/2026-11-09-user-mapping-in-users-yaml.md`).
