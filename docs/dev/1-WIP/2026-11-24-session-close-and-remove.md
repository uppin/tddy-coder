# Changeset: session-close-and-remove

**Type:** Feature
**Packages:** `tddy-service`, `tddy-session-lifecycle`, `tddy-agent-launch`, `tddy-worktree-service`,
`tddy-session-agents`, `tddy-daemon-kernel`, `tddy-changeset`, `tddy-session-activity`,
`tddy-client-web`, `tddy-session-web`
PRDs (target): [`docs/ft/web/session-drawer.md`](../../ft/web/session-drawer.md) § Closing a session,
[`docs/ft/daemon/session-liveness.md`](../../ft/daemon/session-liveness.md) § Closing a session
Design brief: the orchestrator-approved proposal (local `plans/session-close-and-remove-design.md`, not versioned); decisions recorded below.

## Decisions (orchestrator, 2026-11-24)

- Commits no remote holds **do** warrant the dialog, for every worktree.
- The session record and its branch are **kept**; the session lists as ended, without a worktree.
- × on active rows; a mini trash bin on inactive rows that still hold a worktree.
- A session that started others: the dialog **asks** whether to close them too.
- An unreadable worktree: the dialog shows why and offers **Retry**; no "close anyway".
- The split session's codebase half is checked and closed (forwarded to its daemon).
- DeleteSession's three gaps are fixed in this PR; one PR, not a stack.
- (later) A running close shows a mini spinner on its row; the operator can start another close
  right away — closes are concurrent and never block the drawer.

## Problem

An operator who is done with a session has no single action that ends it **and** gives back its
disk. Terminate (`SignalSession` SIGTERM) ends only the main agent; every other tab — tmux tabs,
subagent conversations — keeps running, and the worktree stays. Delete removes the worktree but
only for a session that is already inactive, never asks about uncommitted work, and has three
gaps of its own:

1. **Non-main tmux tabs survive** `DeleteSession`/`SignalSession`: `cli_manager.stop_session`
   cancels only each tab's tmux *client*; nothing reads `tabs.yaml` to kill the tab tmux sessions,
   locally or on a target.
2. **Open subagent conversations are not cancelled** on delete (`forget_session_state` never
   touches `OpenAgentConversations`).
3. **Agent clones are missed after a daemon restart**: `SessionAgentCloneStore` is in memory only,
   while `.session.yaml agents[]` records `daemon_instance_id` + `codebase_session_id` for each.

## State A

| Piece | Where |
|---|---|
| Terminate | `SignalSession` → `LaunchSessions::terminate_session_dir` (tmux-aware since #1024), main agent only |
| Delete | `LaunchSessions::delete_session_as` (`tddy-agent-launch/src/svc_signal_delete_session.rs`) |
| Per-tab stop | `CliManagerTerminalRoster::close_tab` / `end` (`svc_terminal_ports/recorded_tabs.rs`), refuses `main` |
| Dirty reader | `worktree_sync::dirty_files` — local only |
| Target removal | `target_worktrees::remove_target_worktree` — fails on a worktree that is already gone |
| Drawer row | `SessionDrawerItem` — no per-row action |

## State B

### Daemon — `session.SessionService`

```proto
rpc PreviewCloseSession(PreviewCloseSessionRequest) returns (CloseSessionPreview);
rpc CloseSession(CloseSessionRequest) returns (stream CloseSessionEvent);
```

**What a close covers.** Every agent tab of the session — main (tmux-aware), every tab in
`tabs.yaml` and every live terminal (tmux tabs local and on a target, jailed tabs), every open
subagent conversation of the session (local and forwarded) — and every worktree the session owns:

| Owner | Worktree | Removed by |
|---|---|---|
| `SESSION` | `repo_path`, local or on the session's target | force removal (local) / `remove_target_worktree` (target) |
| `CONVERSATION` | `<session wt>/tmp/subagent-worktrees/<conv>` (clone placement) | `ConversationWorktree::remove` (worktree, branch, base ref) |
| `CODEBASE` | the split session's checkout on its codebase daemon | `CloseSession` forwarded to that daemon for the paired `workspace` session |
| `CLONE` | an agent clone on another daemon (a mirror; holds no unique work, never checked) | the existing clone teardown, from the store **and** `.session.yaml agents[]` |

**What counts as a loss** (per worktree): every `git status --porcelain --untracked-files=all`
entry (staged, unstaged, deleted, renamed, untracked-not-ignored), and every commit on the
worktree's `HEAD` that no remote-tracking ref and not the session's recorded base contains
(`git log HEAD --not --remotes [<base>]`, listed up to 20, counted in full). Read through the
session's `RemoteShell` for a target worktree, by forwarding `PreviewCloseSession` for the
codebase half. A worktree that cannot be read carries `unavailable_reason`.

**Preview.** `needs_confirmation` is true when any worktree has a loss or an
`unavailable_reason`. `fingerprint` is a SHA-256 over every worktree's (owner, path, sorted
losses); an unavailable worktree contributes its reason.

**CloseSession**, run as a `TaskRegistry` task of kind `close_session` (the `StreamStartSession`
pattern: first event `started{task_id, daemon_instance_id}`, typed steps on the stream, each step
also written as a line to the task's `progress` channel):

1. **Check.** Recompute the preview. Any `unavailable_reason` → terminal `needs_confirmation`,
   nothing done (the web offers Retry). A loss and `!confirmed`, or `confirmed` and the fingerprint
   differs from `confirmed_fingerprint` → terminal `needs_confirmation`, nothing done.
2. **Terminate** every tab, one step each. Then verify: no live terminal of the session, no
   recorded tmux session alive (local `tmux has-session`, target through its shell), within a
   bounded wait. A survivor fails its step, **every removal is skipped**, and the outcome names it.
3. **Re-check** (an agent wrote between the dialog and its end): the set at risk is recomputed; if
   it is no longer the accepted fingerprint (or no longer empty, on the no-dialog path) → terminal
   `needs_confirmation`, nothing removed. Running the close again is safe.
4. **Remove** each worktree, one step each, every one attempted. Already gone counts as done.
5. **Record**: a `worktree-removed` marker in the session directory (its content the RFC 3339
   time). Not a `.session.yaml` field: that struct is built literally in ~100 places and refuses
   unknown fields, so an older daemon could no longer read the record. The record and the session branch
   are kept; the session lists as ended with `holds_worktree = false`.

Closes of different sessions run concurrently; a second close of a session whose close is still
running is refused `already_exists`, naming the running task.

Outcome: `ok`, or `failed_steps` (each with subject and message). No step falls back to anything:
an unreachable target or peer fails its step with the reason.

**`SessionEntry.holds_worktree`** — true when the session's type owns a worktree, it records one,
no `worktree-removed` marker is present, and (local) the path exists. `RestoreSessionWorktree`
removes the marker.

**DeleteSession** ends every tab and conversation through the same helper (gaps 1–2), tears down
clones from the record too (gap 3), and treats an already-removed worktree — local or on a target —
as done.

### Web — `tddy-session-web`

- `SessionDrawerItem` gets an **×** (`sessions-drawer-item-close-<id>`) on an active row, and a mini
  **trash bin** (`sessions-drawer-item-remove-worktree-<id>`) on an inactive row with
  `holdsWorktree`. Both run the same close, routed to the owning host.
- Click → `PreviewCloseSession`. The close runs at once when nothing needs confirming and the
  session started no live child; otherwise `CloseSessionDialog` opens:
  - one section per worktree: host, owner, branch, files with their change, unpushed commits;
  - a worktree that could not be read shows its reason and a **Retry** (re-runs the preview);
    Confirm stays disabled while any is unreadable;
  - a session that started others (`orchestratorSessionId` = this session, active or holding a
    worktree) asks **Leave them running** (default) / **Close them too**; choosing the latter adds
    each child's own losses to the dialog, and each child is closed after the parent with its own
    fingerprint;
  - Confirm → `CloseSession(confirmed=true, fingerprint)`. A `needs_confirmation` event re-opens
    the dialog with the new list.
- While running, the row's × / trash bin is replaced by a **mini spinner**
  (`sessions-drawer-item-closing-<id>`, hover text = current step). Closes are tracked per session
  and never block the drawer: another row can be closed right away, closes run concurrently.
  Failed steps land in the drawer notices, naming each step and host; then the list refreshes.

## Implementation notes (choices beyond the brief)

- `holds_worktree` for a local session also requires a daemon-managed worktree (under `.worktrees`):
  a close never removes a client-supplied `repo_path`. Only a type that owns its worktree
  (`worktree_removal_applies_to`) lists one; a worktree already gone is listed with no losses and its
  REMOVE prunes.
- Steps: a CHECK DONE after each check; TERMINATE streams RUNNING, then its final state after the
  survivor check (bounded 10 s); a failed TERMINATE skips every REMOVE, clone teardown and RECORD;
  RECORD runs only when every removal succeeded. Clone teardowns are REMOVE steps (subject = the
  clone's workspace session, host = its daemon).
- The codebase half is closed by one forwarded `CloseSession` per codebase daemon; the peer saying
  "no such session" counts as gone, "no room" is refused (as `svc_paired_codebase_teardown`).
- A `needs_confirmation` ending is task status `Completed(0)`, so a prompt is not a failed task.
- A close keeps the tabs in `tabs.yaml` (unlike `close_tab`), so a Resume after Restore brings them
  back.
- A directory under `tmp/subagent-worktrees` that is not a git worktree is reported unreadable,
  which blocks the close rather than deleting it.
- DeleteSession refuses (`unavailable`) when a tab or conversation cannot be ended; with clones read
  from the record, a restarted daemon with a recorded clone and no common room refuses the delete
  instead of leaking the clone.
- Web: when "Close them too" is chosen and the parent's close fails, no child is closed.

## Tests

| Layer | File | Covers |
|---|---|---|
| Rust integration | `tddy-worktree-service/tests/worktree_loss.rs` | porcelain parsing per change kind (literal `-z` strings), each change kind over a real repo, ignored excluded, unpushed vs remotes and base, the 20-commit list and full count, the target reader over `LocalShell` agreeing with the local one, an unreachable target (`InMemoryShell`) is an error |
| Rust integration | `tddy-worktree-service/tests/target_worktree_actions_acceptance.rs` (extended) | `remove_target_worktree` of a worktree already gone succeeds; of one whose directory vanished, prunes its registration |
| Daemon integration | `tddy-session-lifecycle/tests/close_session.rs` | preview of clean / dirty / committed-not-pushed; unconfirmed dirty close does nothing; confirmed close ends main + tmux tab + conversation and removes worktrees, keeps record and branch; fingerprint drift refused; a surviving tab skips removal; `holds_worktree` false after close; two sessions close concurrently; a second close of a closing session is refused |
| Daemon integration | `tddy-session-lifecycle/tests/close_target_session.rs` | target worktree preview and removal over the stand-in target; unreachable target → `unavailable_reason`, nothing done; delete of a target session whose worktree is already gone succeeds |
| Daemon integration | `tddy-session-lifecycle/tests/delete_session.rs` (extended) | delete kills non-main tmux tab sessions; delete of a closed session, and of one whose worktree was removed outside the daemon, succeeds |
| Cypress component | `tddy-session-web/cypress/component/SessionsDrawerClose.cy.tsx` | × only on active rows, trash only on inactive rows holding a worktree; clean → no dialog; dirty → dialog per worktree; Cancel sends nothing; Confirm sends confirmed + fingerprint; unreadable → Retry; children choice; failure notice; spinner while closing; a second row closes while the first is still running |

## Restructuring

`/pr-wrap` step 3.5 (500 production lines, Rust counted to the first `#[cfg(test)]`). Every move is
engine-driven (`tddy-tools restructure`: `check --deep` clean, then `apply`, compile gate green). Hand
edits are `use` lines only (see the last list). Behaviour is unchanged:
`restructure verify --against HEAD` reports `every statement accounted for` (585957 → 585957), and the
comment-line multiset of both packages is unchanged apart from one intra-doc link the engine re-spelled.

### Module layout

```text
tddy-session-lifecycle/src/connection_service/svc_close_session/
  close_plan.rs              the plan and preview: PlannedWorktree, Removal, ClosePlan, plan_close,
                             own/target/codebase worktrees, fingerprint_of
  worktree_reads.rs    (new) reading one worktree: recorded base/branch, local and target branch,
                             read_local_worktree, with_loss, unavailable, change_name
  conversation_worktrees.rs (new) every clone-placed conversation worktree under a session's own
  close_tabs.rs              ending tabs, and now listing them: tabs_a_close_ends, MAIN_TAB,
                             conversation_tab_id, main_label
  close_task.rs              the task and the run's phases (check, terminate, remove, record)
  close_task/steps.rs  (new) streaming a step: CloseRun::{step, note, send}, progress_line
  close_task/codebase.rs (new) closing a split session's checkout: CloseRun::close_codebase,
                             CodebaseEvents
tddy-session-lifecycle/src/test_util/
  held_agents.rs       (new) HeldMain and TestDaemon::{holds_a_main_agent, serves_a_main_for,
                             stops_holding}; `test_util::HeldMain` stays public through a `pub use`
tddy-worktree-service/src/
  worktrees.rs               listing, path policy, remove and clean, and a named `pub use` facade
                             for everything that left, so every `worktrees::…` path still resolves
  worktree_sizes.rs    (new) seam B of the record: the size calculator and its sizes file
  worktree_stats_cache.rs (new) seam A's cache half: WorktreeStatsCache and its cache file
  worktree_diff.rs     (new) seam C: numstat, diff rows, project_worktree_snapshot
  service_helpers.rs         + forget_a_recorded_removal (this PR's helper, beside the other
                             error-to-Status mappers)
```

### Production lines

| File | Merge base | This PR before | After |
|---|---|---|---|
| `svc_close_session/close_plan.rs` | — | 642 | **409** |
| `svc_close_session/close_task.rs` | — | 606 | **449** |
| `tddy-worktree-service/src/worktrees.rs` | 1006 | 1062 | **409** |
| `tddy-worktree-service/src/service.rs` | 679 | 692 | **681** 🔴 still over |
| `tddy-session-lifecycle/src/test_util.rs` | 556 | 581 | **489** |
| `svc_close_session/close_tabs.rs` | — | 254 | 313 |
| `svc_close_session/worktree_reads.rs` | — | — | 118 |
| `svc_close_session/conversation_worktrees.rs` | — | — | 76 |
| `svc_close_session/close_task/steps.rs` | — | — | 76 |
| `svc_close_session/close_task/codebase.rs` | — | — | 99 |
| `test_util/held_agents.rs` | — | — | 98 |
| `worktree_sizes.rs` | — | — | 400 |
| `worktree_stats_cache.rs` | — | — | 155 |
| `worktree_diff.rs` | — | — | 134 |
| `service_helpers.rs` | 147 | 147 | 158 |

`service.rs` keeps the two `forget_a_recorded_removal(&session_dir)?;` calls inside the
`impl WorktreeService` trait impl, which the engine cannot cut (record
`packages/tddy-worktree-service/docs/code-issues/oversized-file-service.md`). Two code-issue records
closed on the number: `oversized-file-worktrees.md` (final 409) and `oversized-file-test-util.md`
(final 489). Both are deleted here.

- [`2026-11-20-session-liveness-oversized-files-after-the-stack.md`](../todo/2026-11-20-session-liveness-oversized-files-after-the-stack.md):
  **✅ RESOLVED HERE**. Its "What closes it" is the `test_util/held_agents.rs` split, done above
  (556 → 489). The other files it lists carry records of their own.

### Engine findings

- **Refused:** the whole step-member run of `CloseRun` (`running` … `send`). `CloseRun::failed` names
  both the method and the field `failed`, and `move_impl_members` refuses the `<CloseRun>::failed`
  spelling. The narrower seam `step`/`note`/`send` was taken instead. See
  [`2026-11-24-restructure-cannot-anchor-a-method-named-like-a-field.md`](../todo/2026-11-24-restructure-cannot-anchor-a-method-named-like-a-field.md).
- **Hand `use` fixes after the moves** (the tree compiled every time):
  - three unused `use sha2::Digest;` deleted, in `worktree_reads.rs`, `conversation_worktrees.rs` and
    `close_tabs.rs` ([`…-tidy-leaves-unused-trait-imports.md`](../todo/2026-11-24-restructure-tidy-leaves-unused-trait-imports.md));
  - three header `#[cfg(test)] use …` moved into their file's `mod tests`, in `close_plan.rs` and
    `worktrees.rs` ×2. They stopped the production-line count at lines 12 and 2
    ([`…-tidy-writes-cfg-test-imports-above-production-code.md`](../todo/2026-11-24-restructure-tidy-writes-cfg-test-imports-above-production-code.md));
  - `service.rs`: the `use crate::service_helpers::forget_a_recorded_removal;` the move wrote below a
    `pub use` was folded into the existing `service_helpers` group.

## Refactoring Needed

### From /red (TDD Red Phase)
- [ ] `close_target_session.rs` re-builds the stand-in-ssh daemon of `remote_target_session_acceptance.rs`
  (`ADaemonWithATarget`, the stub agent, the start request): extract one shared `target_session_support`
  module for both suites.
- [ ] A `git(dir, args)` helper is now written out in `tmux_agent_support/close_session.rs`,
  `worktree_loss.rs`, `worktree_sync_acceptance.rs` and `target_session_support`: one in
  `tddy-testing-commons`.
- [ ] `worktree_loss.rs`'s `ASessionWorktree` repeats `worktree_sync_acceptance.rs`'s `a_repo_world`
  (bare origin + tracking clone): share the builder.
- [ ] `close_events_support` (the `CloseEvents` reader) is reached from `tmux_agent_support` by a
  `#[path]` module so the target suite can declare it alone; fold it into a shared target/tmux support
  crate module if more suites need it.
- [ ] Subagent-conversation coverage of a close is not in the lifecycle suites: conversations are opened
  only through `tddy-daemon-rpc`'s harness. Add a close case there (`session_agent_conversation_acceptance.rs`).
- [ ] Web: `closeSessionPage.revealEndedRow` copies the More-fold logic of
  `sessionsDrawerPage.selectEndedSession`; extract one shared "open the fold this row is under"
  helper and have both use it.
- [ ] Web: `aSessionServiceBackend`'s builders still take `Partial<…Init>` overrides, which `tsc`
  rejects (pre-existing errors at `aSessionEntry` / `aProjectEntry`); `closeSessionBackend.ts`
  avoids the pattern, and the rest of the file should follow it.
