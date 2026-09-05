# 2026-08-18 — **session agent roster

**Type:** Feature

`--fastcontext-*` flags deleted; coder-hosted sessions subscribe to the roster** — `run.rs` drops `--fastcontext-url`/`--fastcontext-model`/`--fastcontext-max-turns` and their `Config` fields; `create_backend` builds `SpecializedAgentBackend` from the def alone. `session_participant/mod.rs` subscribes coder-hosted sessions (tool, cursor-cli) to the roster so they are not a blind spot, as they already do for activity reporting. Feature [session-agent-roster.md](../../../../docs/ft/daemon/session-agent-roster.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-coder)
