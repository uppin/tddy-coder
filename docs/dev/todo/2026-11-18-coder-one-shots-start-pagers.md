# 2026-11-18 — A Coder one-shot's stdout is a tty, so git and other commands start a pager

**Category:** Known defect
**Source:** the Coder `remote-dev` smoke test of 2026-11-18 (orchestrator session `01a1ecee-e942-7982-abfc-104c6c4baa98`, diagnostics session `01a1ecfc-0fac-77a0-bb0d-ac20a90593ae`), left open by the fix that made Coder one-shots cross the pty intact ([`2026-11-18-coder-one-shots-cross-the-pty-intact`](../changesets/2026-11-18-coder-one-shots-cross-the-pty-intact.md))

`coder ssh` gives the command a pty, so a command that pages when stdout is a terminal does so. A
live `git log -1` on `remote-dev` came back wrapped in `less`'s escape sequences. A one-shot whose
output is parsed, or whose pager waits for a key until the deadline, is exposed.

**Why deferred:** no production one-shot was seen paging; found while verifying the pty fix.

**What would close it:** have the one-shot wrapper export `PAGER=cat GIT_PAGER=cat` (and
`GIT_TERMINAL_PROMPT=0`), pinned by a stand-in whose stdout is a tty.
