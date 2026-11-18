# 2026-11-18 — Claude started on a target stops at its folder-trust and project-MCP dialogs

**Category:** Known defect
**Source:** the Coder `remote-dev` smoke test of 2026-11-18 (orchestrator session `01a1ecee-e942-7982-abfc-104c6c4baa98`, diagnostics session `01a1ecfc-0fac-77a0-bb0d-ac20a90593ae`), left open by the fix that made Coder one-shots cross the pty intact ([`2026-11-18-coder-one-shots-cross-the-pty-intact`](../changesets/2026-11-18-coder-one-shots-cross-the-pty-intact.md))

None of the trusted folders in `remote-dev`'s `~/.claude.json` lies under
`~/.tddy/repos/<repo>`, where target worktrees are cut. An interactive Claude start there would show
the folder-trust dialog, and no one is watching that pty to answer it.

**Confirmed live 2026-11-18** (`mantasi/remote-dev`). An unattended Claude start in a qape-hq
worktree there stops at **"Quick safety check: Is this a project you created or one you trust?"**
(default answer *No, exit*). Once trusted, it stops again at **"13 new MCP servers found in this
project"**, from qape-hq's `.mcp.json`. Both were answered from a test (Yes; Esc to reject all), and
the turn then completed. The answer is kept per repository, not per worktree: a later, fresh worktree
of the same clone showed neither dialog.

**Why deferred:** whether the daemon pre-trusts a target's clone, answers the dialogs, or refuses to
start until an operator has, is a product decision. A start cannot reach it today without the binary
workaround of
[2026-11-18-a-target-launch-runs-the-daemon-hosts-agent-path.md](2026-11-18-a-target-launch-runs-the-daemon-hosts-agent-path.md).

**What would close it:** pre-trust the target's worktree root, or answer the prompt during the start,
proven by a start on `remote-dev`.
