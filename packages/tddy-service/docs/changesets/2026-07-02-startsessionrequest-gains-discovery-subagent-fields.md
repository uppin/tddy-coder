# 2026-07-02 — `StartSessionRequest` gains discovery-subagent fields

**Type:** Feature

`discovery_subagent`/`fastcontext_url`/`fastcontext_model`/`fastcontext_max_turns`/`subagent_replaces` (fields 17–21; backward-compatible, old clients get empty/0 defaults) let a `StartSession` caller wire a discovery subagent into a daemon-hosted sandboxed session. Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-service, tddy-daemon)
