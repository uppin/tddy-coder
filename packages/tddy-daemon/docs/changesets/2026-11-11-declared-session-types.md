# 2026-11-11 — Session types come from YAML declarations

**Type:** Fix

- `tests/host_registry_load.rs`: an override that drops its built-in's `extensions.session` fails the daemon's start with the load error.

Cross-package: [declared session types](../../../../docs/dev/changesets/2026-11-11-declared-session-types.md). Product: [Session types are declared](../../../../docs/ft/daemon/session-agent-roster.md#session-types-are-declared).
