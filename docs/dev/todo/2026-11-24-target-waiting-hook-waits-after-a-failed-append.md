# 2026-11-24 — A target's waiting hook waits out its whole answer wait after its line was never logged

**Category:** Future enhancement
**Source:** `fix/target-hooks-log-relative-path` ([change record](../changesets/2026-11-24-target-hooks-log-absolute-path.md)); left out of that PR by the operator, who chose to make every hook write instead of changing how a failed write ends.

`remote_answering_hook_command` (`packages/tddy-agent-backend/src/remote_hook_command.rs`) — the
command a target installs for `PermissionRequest` and for `PreToolUse` matched to
`AskUserQuestion` — appends its `<event>@<nonce>` line and then polls `<worktree>/.tddy/answers/<nonce>`
once a second for up to the spec's answer wait plus `AWAIT_CLIENT_MARGIN_SECS` (about two minutes
for Claude). The append and the wait are joined with `;`, so when the append fails the command still
waits the whole time for an answer that cannot come: no follower saw a line, so nobody writes one.
It then ends `exit 0` with no output, and only then does Claude ask in its own terminal.

What the agent's user sees is a two-minute stall on every permission prompt and every question, with
the append's error (`cannot create …: Directory nonexistent`, from `sh`) on the hook's stderr, which
Claude Code shows only in its transcript view.

The append now uses the worktree's absolute log path, which the launch creates before the agent
starts, so this needs something else to fail first — the agent deleting `.tddy` (`git clean -fdx`
removes excluded files), a full disk, a changed owner. It is no longer the common case, which is why
it was left out.

Shape of the fix, for the operator to decide: end the waiting command as soon as its append fails,
without waiting — `printf … >> LOG || exit <code>`. The exit code is the decision: Claude Code reads
`2` as a blocking error (the prompt or question is refused), any other non-zero code as a
non-blocking one (Claude asks in its terminal and shows the error), `0` as no decision. Whatever is
chosen should be pinned with a test under `dash`, the `/bin/sh` of a Debian or Ubuntu target, which
exits a failed redirection `2` where macOS's `sh` exits `1`.
