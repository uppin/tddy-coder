# 2026-11-05 — What the generic CLI launch does not yet do like the bespoke claude path

**Category:** Gap
**Owner:** `#agent-tools` n17 `cli-dispatch-collapse` ([#823](https://github.com/uppin/tddy-coder/pull/823)) for the bespoke-path items; n19 `agent-config-removal` for the renderer env.
**Source:** `#agent-tools` 14/19 (#820)

> **Updated at `#agent-tools 17/19` green.** The bespoke tmux paths and the late-minted chat are
> resolved (bullets 1 and 4 below stay as history); the managed environment is carried on the tool
> server for a spec whose `mcp` slot names a file (`cli_agent_steps::carry_on_the_tool_server`) and
> still reaches no one for a spec whose slot is `placed` in the agent's home (cursor) or `unsupported`;
> the renderer variable is applied by `harness_from_spec::renderer_env` until n19 moves it into
> `run.env`.
>
> **Updated at `#agent-tools 17/19` wrap.** A `tddy-tools` the agent runs from its own shell no longer
> depends on the tool-server registration: the managed socket and the operator-notify session are
> written into the worktree's `.tddy/session.json` pointer (`tddy-agent-launch::cli_session_pointer`),
> so a managed agent whose spec registers no MCP server (or `placed`s it in the agent's home) reaches
> its controller through `tddy-tools` on its shell. Its MCP server still gets them only through a
> file-placed registration. The pointer's ssh gaps are
> [2026-11-06-ssh-session-pointer-exclude-and-removal](2026-11-06-ssh-session-pointer-exclude-and-removal.md).

- **Bespoke tmux start and resume** (`claude_cli_spawn_steps::spawn_claude_cli_process`, `svc_resume_claude_cli_session`) still build the hand-built argv; their two `TODO(agent-tools-cascade)` markers stay until n17 deletes both paths. The generic tmux start and resume render through `plan_host_pty` for every agent.
- **Managed and role environment.** A `TDDY_*` pair a managed workflow (`TDDY_SOCKET`, `TDDY_SESSION_DIR`, `TDDY_REPO_DIR`) or a project-home role produces is carried on the agent's tool-server registration (`mcp` env), never on the agent process (stack rule). A managed agent whose spec declares no `mcp` slot therefore has no way to reach its controller; the bespoke path put them on the process.
- **Minted chat recorded late.** A generic resume of a `minted` conversation whose session recorded no chat id is refused; the bespoke `resume::chat_to_resume` mints one then.
