# 2026-07-21 — Replaces-driven allow/disallow builders + session-action tools

**Type:** Feature

`build_claude_allowlist`/`build_claude_disallowlist` derive everything from the defs' replaced-tool set: differently-named native aliases are hard-disabled per replacement (`native_aliases`: `Bash`/`BashOutput`/`KillShell` for `Shell`; `Edit`/`MultiEdit`/`NotebookEdit` for `Write`), and a replaced `Shell` (`shell_is_replaced`) swaps in the three `mcp__tddy-tools__{request_action,list_actions,invoke_action}` session-action tools. Feature [no-bash-mode.md](../../../../docs/ft/coder/no-bash-mode.md). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). PR [#308](https://github.com/uppin/tddy-coder/pull/308). (tddy-sandbox-recipes)
