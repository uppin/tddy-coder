# 2026-07-21 — Session-action MCP tools for a replaced Shell

**Type:** Feature

new `action_tools.rs`: `request_action` runs the Shell-replacing author subagent (`shell_replacing_author` from `TDDY_SUBAGENTS_JSON`) in a bounded 3-attempt correction loop (64 KiB cap, in-jail pre-validation) and dispatches `EstablishAction` to the host; `list_actions`/`invoke_action` are host round-trips. The exec catalog filters replaced tools as before; the action router merges only when a def replaces `Shell`. Feature [no-bash-mode.md](../../../../docs/ft/coder/no-bash-mode.md). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). PR [#308](https://github.com/uppin/tddy-coder/pull/308). (tddy-tools)
