# 2026-07-22 — Session catalog: per-session SQLite index of actions + build targets (populate)

- Each session builds a per-session SQLite catalog (`<session_dir>/catalog.db`) on worktree-open, unifying the session's action manifests with the repository's auto-discovered `BUILD.yaml` build targets ([session-catalog.md](../session-catalog.md)).
- Entries are stored as JSON with a projected, indexed `package` column — the first supported query is "list targets for a package".
- Population runs asynchronously as a tracked task; build-target discovery is contributed by `tddy-coder` through a `BuildCatalogProvider` port, so `tddy-core` keeps no build-system dependency.
- This ships the producer only: serving `list-actions` from the catalog (the read-path cutover) and the daemon-managed populate trigger land in a later change; `list-actions` currently still reads YAML directly.
