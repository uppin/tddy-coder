# 2026-11-18 — Claude started on a target will probably stop at the folder-trust prompt

**Category:** Known defect (expected, unverified)
**Source:** the Coder `remote-dev` smoke test of 2026-11-18 (orchestrator session `01a1ecee-e942-7982-abfc-104c6c4baa98`, diagnostics session `01a1ecfc-0fac-77a0-bb0d-ac20a90593ae`), left open by the fix that made Coder one-shots cross the pty intact ([`2026-11-18-coder-one-shots-cross-the-pty-intact`](../changesets/2026-11-18-coder-one-shots-cross-the-pty-intact.md))

None of the trusted folders in `remote-dev`'s `~/.claude.json` lies under
`~/.tddy/repos/<repo>`, where target worktrees are cut. An interactive Claude start there would show
the folder-trust dialog, and no one is watching that pty to answer it.

**Why deferred:** unverifiable until a Claude start on a target gets past its launch
([2026-11-18-a-target-launch-runs-the-daemon-hosts-agent-path.md](2026-11-18-a-target-launch-runs-the-daemon-hosts-agent-path.md)).

**What would close it:** pre-trust the target's worktree root, or answer the prompt during the start,
proven by a start on `remote-dev`.
