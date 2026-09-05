# 2026-07-21 — `session_actions::validate_authored_manifest`

**Type:** Feature

shared field checks for subagent-authored manifests (non-empty argv, filename-safe id, compilable `input_schema`), used by both the in-jail `request_action` retry loop (tddy-tools) and the authoritative host-side `EstablishAction` handler (tddy-sandbox-app). Feature [no-bash-mode.md](../../../../docs/ft/coder/no-bash-mode.md). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). PR [#308](https://github.com/uppin/tddy-coder/pull/308). (tddy-core)
