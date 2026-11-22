# 2026-11-22 — Orchestrator ↔ target-session communication: reports, activity paging, the start's branch, colours, attachments, answered questions

**Type:** Fix · **PR:** #997 · **Packages:** `tddy-agent-launch`, `tddy-session-activity`,
`tddy-tool-engine`, `tddy-changeset`, `tddy-tools`, `tddy-session-files`, `tddy-session-split`,
`tddy-pty`, `tddy-daemon-rpc` · Feature:
[project-concept.md § Reporting to the orchestrator](../../ft/daemon/project-concept.md#reporting-to-the-orchestrator),
[§ Following a session](../../ft/daemon/project-concept.md#following-a-session)

## Problem

A live probe (2026-11-22) of an orchestrator driving a claude-cli child on the Coder workspace
`remote-dev` found seven gaps. Root causes, measured against the installed daemon, its log, the
project log and the remote worktree:

1. **A target child's reports were lost.** The guidance piped `{"text": …}` into
   `remote_hook_command`, whose `$(cat | tr -d '\n')` is expanded by zsh — the workspace's login
   shell — before stdin is the pipe, since zsh runs a pipeline's last element in the current shell.
   Both reports reached `.tddy/hooks.log` with an empty payload; the follower dropped them as
   `BrokenPayload`, silently. `ReadSessionMessages` meanwhile reported `lost: 934`: every evicted
   project record after the reader's position 0, none of them a message.
2. **`ReadSessionActivity` could not page.** It paged by `activity_seq`, which every persisted row
   carries as 0 (0 of 38,290 rows on the probe host had another value): that field is the session
   room's tick, stamped only on the room's in-memory broadcast. Coalescing by `call_id` replaced the
   whole row, so every finished call lost its start time.
3. **`GetSessionStart` dropped the branch on success.** A target cut never wrote
   `changeset.branch`, so the session listed with `branch: ""`, and the ledger took `Some("")` over
   the branch the start had reported.
4. **No `TDDY_SESSION_DIR` in a child's shell** — by design (operator, 2026-11-22): `TDDY_*` stays
   off the agent process, and on a target the session directory is on the daemon's host.
5. **Nothing named a session's attachments** — the probe's first move was `find /`.
6. **A target terminal had no colour.** The agent ran under `env -i` with no `TERM`.
7. **An orchestrator's answer to `AskUserQuestion` was accepted, then dropped.** When the waiting
   `PreToolUse` hold ended unanswered, Claude fired `PermissionRequest` for the same call; that hook
   held it as a new question, and its answer was mapped as a permission, which it is not.

## What changed

- **`tddy-agent-launch`**: the hook-log back channel runs `remote_hook_command` under `sh -c`; the
  follower logs every dropped line with its reason; a target agent's `env -i` names
  `TERM=xterm-256color COLORTERM=truecolor`; a remote cut records its branch in the changeset and
  copies the session's attachments to `.tddy/attachments/` (`remote_attachments`, git-excluded); a
  target's waiting `PermissionRequest` of `AskUserQuestion` is answered as a question.
- **`tddy-session-activity`**: `question_decision_json_for(event, …)`; `messages_lost_after` and
  `evicted_messages.rs` (`lost` counts the reader's evicted messages only); the unknown-request
  refusal no longer calls a question a permission request.
- **`tddy-changeset`**: `read_all_sequenced_agent_activity` → `SequencedActivity { log_seq, record }`
  (the line of a call's latest row); field-wise coalescing keeps the start time and input.
- **`tddy-tool-engine`**: `page_activity` pages `SequencedActivity` by `log_seq` (a call that
  finishes is handed over again); `apply_ending` treats an empty listed branch or worktree as none.
- **`tddy-tools`**: `session-hook --await-answer` answers a `PermissionRequest` of `AskUserQuestion`
  with `updatedInput`.
- **`tddy-session-files`** / **`tddy-session-split`**: `attachment_prompt` names every materialized
  attachment in a CLI start's first prompt, by a path its agent can read.
- **`tddy-pty`**: `terminal_variables::TERMINAL_VARIABLES`, shared with the target launch.
- **`tddy-daemon-rpc`**: the `ReadSessionActivity` port reads sequenced activity.
- `flake.nix` declares `pkgs.zsh` for the zsh back-channel suite.

A label beside `other` on a single-choice answer stays refused: it is Claude's single-choice
dialog, where Other replaces a choice.

## File length (`/pr-wrap` step 3.5)

| File | Production lines (base → wrap) | Now |
|---|---:|---|
| `tddy-session-activity/src/project_notification_log.rs` | 497 → **499** | the evicted-message bookkeeping, the window's push-and-evict and the restore split are in `project_notification_log/evicted_messages.rs` |
| `tddy-pty/src/runtime.rs` | 533 → **529** | `TERMINAL_VARIABLES` and `set_terminal_variables` are in `terminal_variables.rs`; `oversized-file-runtime` narrowed, still open |

## Still open

[`docs/dev/todo/2026-11-22-coder-target-back-channel-and-activity-cursor.md`](../todo/2026-11-22-coder-target-back-channel-and-activity-cursor.md):
the live re-run of the probe, and the stack child's changeset prompt line's path.
