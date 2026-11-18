# 2026-11-18 — A Coder one-shot past its deadline leaves its process running on the workspace

**Category:** Known defect
**Source:** the Coder `remote-dev` smoke test of 2026-11-18 (orchestrator session `01a1ecee-e942-7982-abfc-104c6c4baa98`, diagnostics session `01a1ecfc-0fac-77a0-bb0d-ac20a90593ae`), left open by the fix that made Coder one-shots cross the pty intact ([`2026-11-18-coder-one-shots-cross-the-pty-intact`](../changesets/2026-11-18-coder-one-shots-cross-the-pty-intact.md))

At the deadline `CoderShell::exec` drops the local `coder ssh` (`kill_on_drop`). The remote
`sh` and its children are not signalled and keep running. The hung settings writes of the smoke test
left two `sh`/`cat` pairs alive on `remote-dev` (pids 4098675/4098677 and 45192/45194), still
there hours later. The write no longer hangs, but any one-shot that runs past its deadline leaks the
same way.

**Why deferred:** not needed to unblock starts once writes stopped hanging.

**What would close it:** bound the command on the workspace too: a remote `timeout` wrapper a little
inside the local deadline, or a process group killed by a second one-shot. Pin it with a stand-in
whose command outlives the deadline and records whether it was killed.
