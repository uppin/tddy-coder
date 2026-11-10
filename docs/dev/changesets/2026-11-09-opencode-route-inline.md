# 2026-11-09 — Every OpenCode launch carries its route inline

**Type:** Refactor

`#agent-tools` 29/32 ([#857](https://github.com/uppin/tddy-coder/pull/857)).

A new route shape, `inline_config`, carries a provider route's generated JSON configuration inline, in the variable the spec's `route.config_env` names. The route body (`inline_config_route`) puts it in the route's plain environment, which every launcher already applies, so no launcher names an agent or a variable. OpenCode's spec declares `shape: inline_config`, `config_env: OPENCODE_CONFIG_CONTENT` and no hooks adapter. Every OpenCode launch — the daemon's ACP subagent, `tddy-coder --agent opencode`, and `opencode-cli` on the PTY, jailed or not — carries the route inline; no route file is written and nothing sets `OPENCODE_CONFIG`.

Packages: tddy-agent-backend (`RouteShape::InlineConfig` and `word()`, `RoutedAgent.config_env`, `inline_config_route`; the `opencode-route-file` hooks adapter deleted), tddy-coding-agent-specs (`route_slot_errors` load validation, `opencode.yaml`), tddy-session-lifecycle (the daemon ACP planner loses its `json_config_file` branch and refuses a route carrying files, naming its shape), tddy-daemon-sandbox (`route_hosts` reads URLs inside a route variable's value), tddy-coding-subagent (`OPENCODE_CONFIG_CONTENT` constant deleted). Docs: `supervisor.yaml.production`, `tddy-supervisor.md`.

`route.shape: json_config_file` stays for a drop-in PTY or jailed agent; no built-in declares it.

Backlog: `docs/dev/todo/2026-11-04-opencode-acp-route-file-has-no-session-owner.md` closed as **file not needed** (user, 2026-11-07) and deleted: no secret is in the configuration; inline configuration outranks the worktree's `opencode.json` where `OPENCODE_CONFIG` ranks below it, so a repository could have redirected `baseURL` and harvested the routed key; and no directory owned by the session user exists.

Code-issue measurements:

- `packages/tddy-coding-agent-specs/src/spec.rs`: 1110 → 1152 production lines (stack-shared, recorded in `oversized-file-spec.md`; deferred to the `#agent-tools` sweep node).

Left for the sweep node: `TODO(sweep)` — key the daemon's `toml_args` renderer off the shape rather than the Codex-named `codex_acp_route_args`, and widen `launcher_names_no_agent` beyond "opencode".
