# 2026-10-22 — Routed jail plans; OpenCode and Muse recipes

**Type:** Feature

`RunnerPlanRequest.route`: the route's secrets become jail secrets (`route_secrets`,
`TDDY_SECRET_<NAME>`), its environment joins the jail's, and its files are written owner-only into
`<scratch>/provider-route` (`route_files.rs`), with the harness's config variable (`OPENCODE_CONFIG`)
naming the first file. A routed plan injects no `CLAUDE_CODE_OAUTH_TOKEN` and opens no OAuth inbound
port. New recipes: `SandboxRecipe::OpenCodeCli` (Bun, so dynamic code generation is allowed) and
`MuseCli` (a native binary, so it is not), detected from `--agent-kind opencode | muse`. Their reads
come from the agent binary the runner argv names (`agent_binary_from_runner_argv`). PR
[#670](https://github.com/uppin/tddy-coder/pull/670) (`#provider-route` 9/9).
