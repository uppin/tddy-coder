# 2026-11-03 — Every jail runs a tool plan

`#agent-tools` 10/12 ([#799](https://github.com/uppin/tddy-coder/pull/799)).

- **`tddy-sandbox-app --agent <id>`** picks the coding agent from the tool registry, including
  `<tddyhome>/tools` overrides. It launches the agent the way the daemon does: argv and slots from
  the spec, and home, files, secrets, policy and egress from the tool definition. An unknown id is
  refused with the registry's ids listed. `opencode` and `muse` are still refused, because they
  need a provider route.
  - `--agent-kind`, `--claude-binary`, `--cursor-binary`, `--claude-home-dir` and
    `--cursor-home-dir` are gone, along with their config keys.
  - The binary comes from the spec's override (`TDDY_CLAUDE_BINARY`), and the jail home is
    `<tddyhome>/homes/<key>`.
- **Split sessions and the sandboxed-codebase host agent** run `claude.yaml`'s `host_agent` variant
  (`--strict-mcp-config --setting-sources user`, every native filesystem and shell tool withdrawn,
  `LS` included). A split session now also drops the checkout's settings.
- **The sandbox runner and recipes know no coding agent.** The runner runs an exec plan, a confined
  action or a workspace-tools jail. It refuses any other start, and every agent flag is a parse
  error. The plan, not the argv, says which directory stays read-only and whether the jail talks
  over stdio.

See [Sandbox builder](../sandbox-builder.md) and
[Sandboxed codebase mode](../sandboxed-codebase-mode.md).
