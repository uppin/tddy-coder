# Changesets Applied

Wrapped changeset history for tddy-daemon.

- **2026-03-28** [Feature] ListSessions workflow enrichment — `session_list_enrichment` reads `.session.yaml` and `changeset.yaml`; `SessionEntry` gains `workflow_goal`, `workflow_state`, `elapsed_display`, `agent`, `model`; `list_sessions` runs `session_reader` + enrichment in `spawn_blocking_with_timeout`; enrichment errors logged at warn. Integration test `list_sessions_enriched`. See [connection-service.md](./connection-service.md). (tddy-daemon, tddy-service proto, tddy-web)
- **2026-03-23** [Feature] Root `./install --systemd` — Copies release binaries, optional web bundle, production config when absent, generated systemd unit; ENV path overrides and `INSTALL_NO_SYSTEMCTL` for tests. Documented in [docs/ft/daemon/systemd-install.md](../../../docs/ft/daemon/systemd-install.md). (tddy-daemon, tddy-e2e, repo root)
- **2026-03-21** [Feature] StartSession `agent` — `StartSession` passes `agent` into spawn; `spawner` / `spawn_worker` add `--agent` when set for `tddy-coder`. (tddy-daemon)
- **2026-03-19** [Feature] tddy-daemon Binary Extraction — New binary crate. DaemonConfig (listen, livekit, github, users, allowed_tools). AuthService from config. ConnectionService: ListTools, ListSessions, StartSession, ConnectSession, ResumeSession. ProcessSpawner with fork+setuid/setgid, LiveKit credential passing. Session reader from ~user/.tddy/sessions. TokenService when LiveKit configured. serve_web_bundle via tddy-coder. (tddy-daemon)

- **2026-03-21** [Feature] Project concept — `repos_base_path` config; `project_storage` (`~/.tddy/projects/projects.yaml`); `ListProjects`, `CreateProject` (optional `user_relative_path`); `StartSession` by `project_id`; `clone_as_user` and spawn-worker `clone` requests; `SessionMetadata`/`SessionEntry` carry `project_id`. See [connection-service.md](./connection-service.md). (tddy-daemon)
- **2026-03-24** [Feature] DeleteSession — `DeleteSession` removes an inactive session directory under the caller’s sessions tree; `session_deletion` validates ids and path containment; generic internal error on `remove_dir_all` failure with full detail in logs. (tddy-daemon)
