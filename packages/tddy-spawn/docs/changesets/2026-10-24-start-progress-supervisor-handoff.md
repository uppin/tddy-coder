# 2026-10-24 — Supervisor handoff

`#start-progress` 3/8 ([#736](https://github.com/uppin/tddy-coder/pull/736)). `SpawnBackendChoice` is decided from the environment (`from_environment` / `adopt_from_environment`), not from config; the caller consumes the variable pre-runtime. See [spawn worker](../spawn-worker.md). Cross-package entry: [docs/dev/changesets/2026-10-24-start-progress-supervisor-handoff.md](../../../../docs/dev/changesets/2026-10-24-start-progress-supervisor-handoff.md).
