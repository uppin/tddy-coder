# 2026-11-02 — A sandboxed Claude CLI session is launched from its tool definition

`#agent-tools` 5/12 ([#794](https://github.com/uppin/tddy-coder/pull/794)).

- **The jail is built from `claude.yaml`.** Its home, files, secrets, policy and network are the
  definition's, and the start and the relaunch use one launcher. The runner is handed a rendered
  argv, env and files (`--exec-plan`) and no Claude flag.
- **Claude sees an ordinary environment:** `HOME` is `<tddyhome>/homes/claude`, `PATH` holds its
  declared tools, `HTTPS_PROXY` points at the egress shim, and no `TDDY_*` variable is set on it.
- **Network is narrowed from any host to the declared ones** (`api.anthropic.com`,
  `statsig.anthropic.com`, `sentry.io`, `console.anthropic.com`, `claude.ai`), plus a provider
  route's base-URL host. A routed session gets no `CLAUDE_CODE_OAUTH_TOKEN`.
- **The jailed Claude home moved.** `<tddyhome>/sandbox-claude-home` is renamed to
  `<tddyhome>/homes/claude` on first use, credentials kept; both existing refuses the start.
  daemon.yaml `claude_cli.claude_home_dir` and `TDDY_SANDBOX_CLAUDE_HOME` no longer decide the
  daemon's jail home; a custom one elsewhere is not migrated, and the new home is seeded from the
  host's `~/.claude/.credentials.json`. `tddy-sandbox-app` is unchanged.
- **A project-home main can be set up sandboxed** and is listed with `session_role: project-home`.
- A sandboxed start on a host without coding agents installed is `FAILED_PRECONDITION`.

See [Claude CLI session § The jail](../claude-cli-session.md#the-jail-from-the-claude-tool-definition).
