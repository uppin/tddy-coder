# 2026-11-18 — `CoderShell::interactive` carries no `-t`

**Type:** Fix

`CoderShell::interactive` is `coder ssh --disable-autostart <workspace> -- <command>`. coder v2.33.5
has no `-t` flag (`unknown shorthand flag`) and gives the command a terminal when its stdin is one, which
is the caller's PTY. Pinned by `the_interactive_command_opens_a_terminal_on_the_workspace`. See
[architecture.md](../architecture.md). Full record:
[`docs/dev/changesets/2026-11-18-coder-interactive-launch.md`](../../../../docs/dev/changesets/2026-11-18-coder-interactive-launch.md).
