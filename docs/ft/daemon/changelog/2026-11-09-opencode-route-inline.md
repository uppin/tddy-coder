# 2026-11-09 — Every OpenCode launch carries its route inline

OpenCode on a provider account no longer gets a route file.

- The generated OpenCode configuration is carried inline in `OPENCODE_CONFIG_CONTENT` on every launch: the daemon's ACP subagent, `tddy-coder --agent opencode`, and `opencode-cli` sessions, jailed or not. `OPENCODE_CONFIG` is not set and no `opencode.json` is written.
- Inline configuration outranks a worktree's own `opencode.json`, so a repository can no longer redirect the route's endpoint.
- A spec declares this with `route.shape: inline_config` and `route.config_env`. An `inline_config` route without `config_env`, naming a `TDDY_*` variable, or with a `files_dir` fails loading, naming the spec.
- A daemon ACP launch whose route carries a file is refused, naming its shape.
