# 2026-11-22 — Orchestrator ↔ target-session fixes: what is still open

**Category:** Deferred from PR #997 (orchestrator ↔ target-session communication)
**Source:** live probe on 2026-11-22 — orchestrator `01a16dc6-ab4c-7dc3-844e-baca4514f86e`, claude-cli
child `01a201cd-1cd1-7c53-8481-77991a117fac` on the derived Coder host `remote-dev`. The defects it
found, their root causes and their fixes are recorded in
[`../changesets/2026-11-22-coder-target-back-channel.md`](../changesets/2026-11-22-coder-target-back-channel.md).

## 1. The probe has not been re-run against a real workspace

Every fix is pinned by a test against a stand-in (a fake Coder workspace, an in-memory shell,
`zsh -c` on the test host). Nothing has re-run the probe on `remote-dev` itself.

**Why deferred:** it needs a release build installed in Tddy Desktop and a live Coder workspace —
an operator step, not a CI one.

**What would close it:** start a claude-cli child on `remote-dev` from an orchestrator, with one
attachment, and check:

- both of the child's reports arrive in the orchestrator's `ReadSessionMessages`, with `lost: 0`;
- a `ReadSessionActivity` poll from the last `next_cursor` returns only the newer calls, a call read
  while `running` comes back `completed`, and every finished call has a `started_unix_ms`;
- `GetSessionStart` still names the branch after `state: succeeded`;
- `ReadSessionTerminal` with `styles: true` returns colour runs;
- the first prompt names `.tddy/attachments/<file>`, and the file is there;
- an `AskUserQuestion` answered only after its first 120 s wait reaches Claude.

## 2. A stack child's changeset line names a path its agent cannot open

`tddy-session-files` `stack_doc_attachments::attached_changeset_prompt_line` tells a stack child to
read `artifacts/attachments/<name>`. That path is relative to the **session directory**, but the
agent starts in its **worktree**, and on a target it cannot reach the session directory at all.
`attachment_prompt::attachments_prompt_line` names every attachment by a path the agent can read,
but only a CLI start's `attached_initial_prompt` appends it.

**Why deferred:** the stack spawn (`child_spawn_handler`) calls `prompt_with_attached_changeset`
directly, outside `attached_initial_prompt`; changing its wording touches the `#pr-stack` spawn
contract and its tests, which #997 did not otherwise change.

**What would close it:** `attached_changeset_prompt_line` names the changeset by
`AttachmentHome::path_of` for the child's placement — or the stack spawn appends
`attachments_prompt_line` too and the changeset line points at it.
