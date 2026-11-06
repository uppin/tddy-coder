# 2026-11-02 — Claude Code CLI launches from its tool definition

`#agent-tools` 2/12 ([#791](https://github.com/uppin/tddy-coder/pull/791)).

- **An unsandboxed `claude-cli` start and resume launch from `claude.yaml`.** The argv template
  renders the model, session id, permission mode, resume flag and first prompt. The definition's
  `coding_agent` slots write the hooks settings, the appended system prompt, a project-home main's
  `--allowedTools` / `--disallowedTools`, and its `tddy-tools` MCP registration. An ordinary
  session's argv, launch environment and hooks file are byte-for-byte what they were.
- **`<tddyhome>/tools/claude.yaml` replaces the built-in whole.** An argv token added there reaches
  the process. daemon.yaml `claude_cli.binary_path` still overrides the binary.
- **A tool file under `<tddyhome>/tools` that does not load now fails daemon start**, naming the
  file. That covers an unknown field, an unknown slot kind, and an `extensions:` key no owner claims.
- **A hooks file that cannot be written fails the start.** Before, the launch warned and continued.
- The sandboxed `claude-cli` path is unchanged.

See [Claude CLI session § Launch](../claude-cli-session.md#launch-from-the-claude-tool-definition).
