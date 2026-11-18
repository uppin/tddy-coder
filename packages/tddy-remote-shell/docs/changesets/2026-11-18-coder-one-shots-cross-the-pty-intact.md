# 2026-11-18 — A Coder one-shot's stdin and stdout cross the workspace's pty intact

**Type:** Fix

`CoderShell::exec` wraps every one-shot so the command never relies on `coder ssh`'s pty, which turns
no end of the CLI's stdin into an EOF and rewrites `\n` as `\r\n`:

- `stty -opost` runs first, so output arrives as written.
- A command given no stdin reads `/dev/null`.
- Given stdin crosses as counted base64 lines (`head -c`, so no EOF is needed). It is decoded into an
  owner-only temp file, length-checked, and made the shell's stdin, unlinked before the command runs.
  It is written only once the start marker shows echo is off, and it never travels on a command line.
- `base64 = "0.22"` is a dependency.

This fixes the 60 s hang of every Claude start on a Coder workspace, which wrote its settings file
through `write_file`. It is pinned by three new stand-ins and five tests in
`tests/coder_shell_acceptance.rs`, and was run against `mantasi/remote-dev`. See
[architecture.md](../architecture.md). Full record:
[`docs/dev/changesets/2026-11-18-coder-one-shots-cross-the-pty-intact.md`](../../../../docs/dev/changesets/2026-11-18-coder-one-shots-cross-the-pty-intact.md).
