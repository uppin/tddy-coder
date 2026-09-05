# 2026-07-21 — Tool-replacement validation + host-side session-action handlers

**Type:** Feature

`config::validate_tool_replacements` (at most one Shell replacer = the action author; a def replacing `Write`/`StrReplace`/`Delete` must bind the matching internal tool) gates spawn; new `host_actions.rs` handles `EstablishAction` (authoritative re-validation, idempotent for identical bytes, collision error on redefinition), `ListActions`, `InvokeAction` against the host-only session dir; `AppToolHandler` hard-rejects `Shell`/`Await` dispatches when Shell is replaced. Feature [no-bash-mode.md](../../../../docs/ft/coder/no-bash-mode.md). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). PR [#308](https://github.com/uppin/tddy-coder/pull/308). (tddy-sandbox-app)
