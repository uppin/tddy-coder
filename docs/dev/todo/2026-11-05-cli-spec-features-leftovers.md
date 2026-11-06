# 2026-11-05 — What the generic CLI launch does not yet do like the bespoke claude path

**Category:** Gap
**Owner:** `#agent-tools` n17 `cli-dispatch-collapse` ([#823](https://github.com/uppin/tddy-coder/pull/823)) for the bespoke-path items; n19 `agent-config-removal` for the renderer env.
**Source:** `#agent-tools` 14/19 (#820)

- **Bespoke tmux start and resume** (`claude_cli_spawn_steps::spawn_claude_cli_process`, `svc_resume_claude_cli_session`) still build the hand-built argv; their two `TODO(agent-tools-cascade)` markers stay until n17 deletes both paths. The generic tmux start and resume render through `plan_host_pty` for every agent.
- **Managed and role environment.** A `TDDY_*` pair a managed workflow (`TDDY_SOCKET`, `TDDY_SESSION_DIR`, `TDDY_REPO_DIR`) or a project-home role produces is carried on the agent's tool-server registration (`mcp` env), never on the agent process (stack rule). A managed agent whose spec declares no `mcp` slot therefore has no way to reach its controller; the bespoke path put them on the process.
- **Renderer env.** `config.claude_cli_renderer_env()` (`claude_cli.fullscreen`) is not applied by the generic launch; it is config n19 removes.
- **Minted chat recorded late.** A generic resume of a `minted` conversation whose session recorded no chat id is refused; the bespoke `resume::chat_to_resume` mints one then.
