# Changeset: session-close-and-remove

**Type:** Feature
**Packages:** `tddy-service`, `tddy-session-lifecycle`, `tddy-agent-launch`, `tddy-worktree-service`,
`tddy-session-agents`, `tddy-daemon-kernel`, `tddy-session-activity`, `tddy-cli-sessions`,
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

### From validation (2026-11-24)

- **A close holds its session.** While a close runs, `ResumeSession`, a new tab
  (`StartTerminalSession`) and `DeleteSession` of that session are refused `ALREADY_EXISTS`, naming
  the close's task (`SessionCloses::refuse_while_closing`, reached by `tddy-agent-launch` through the
  `SessionClosesInFlight` port of `LaunchSessions`, and by the terminal roster's `start`). Not
  `FAILED_PRECONDITION`: a peer forwarding a delete reads that code as "no such session".
- **An unreadable `tabs.yaml` stops the close at its check** (CHECK failed, every removal and the
  record skipped, nothing ended) and refuses a delete (`INTERNAL`). No `tabs.yaml` is no recorded
  tab. `tabs_beside_main` answers the roster error; each tab is ended through the tmux the roster
  read gave it (`end_with_its_session` no longer re-reads the roster and logs).
- **A codebase half is "gone" only on the exact refusal.** `peer_does_not_hold_the_session` matches
  `FAILED_PRECONDITION` carrying `SESSION_NOT_HELD_HERE` (the words `DeleteSession` and the close
  refuse an absent session with); `held_session` answers an unreadable record `INTERNAL`. Any other
  refusal leaves the codebase worktree unreadable.
- **Presence is read with its error.** Local: `worktrees::path_is_present` (`symlink_metadata`; only
  `NotFound` is absent). Target: `target_worktrees::target_path_is_present`, a probe that answers
  "absent" only when the nearest existing ancestor is a searchable directory. The plan carries
  `present` into `Removal::Local`/`Removal::Target`; a worktree planned gone that is there again is
  refused ("it reappeared since it was checked"). `force_remove_worktree_under_repo` and
  `remove_target_worktree` check afterwards that the worktree is neither on disk nor listed.
- **A torn-down clone leaves the record.** `tear_down_agent_clone` removes every `agents[]` entry
  reading the clone (`SessionAgentRosterStore::forget_clone`) and republishes the roster, so close
  and delete share it. `every_agent_clone_of` answers an unreadable record as an error; no directory
  or no `.session.yaml` records no clone.
- **A conversation worktrees' directory that cannot be listed** is one unreadable conversation
  worktree (`NotFound` is none), which blocks the close.
- **A closed target session is deleted without its target.** With the `worktree-removed` marker
  present, `DeleteSession` does not ask the target to kill the main's tmux session nor any tab's
  (`SessionTab::without_its_target_tmux`): the close ended them and checked them gone. A local tmux
  kill still runs (cheap, needs nothing remote).
- **Survivor checks read "cannot tell" as surviving**: a local tmux that cannot be asked
  (`tmux_session_lives`, `tddy-cli-sessions`), a workspace jail registered again, and a local
  conversation whose interrupted turn still holds its session (`ClosedTurnLoop::turn_still_runs`,
  from `close_every_conversation_of`).
- **Nested removals**: once a conversation worktree inside the session's own worktree fails to go,
  the session's worktree is skipped rather than forced out over it.
- The progress note of a re-check that stops the close says the tabs were ended and no worktree was
  removed.

## Tests

| Layer | File | Covers |
|---|---|---|
| Rust integration | `tddy-worktree-service/tests/worktree_loss.rs` | porcelain parsing per change kind (literal `-z` strings), each change kind over a real repo, ignored excluded, unpushed vs remotes and base, the 20-commit list and full count, the target reader over `LocalShell` agreeing with the local one, an unreachable target (`InMemoryShell`) is an error |
| Rust integration | `tddy-worktree-service/tests/target_worktree_actions_acceptance.rs` (extended) | `remove_target_worktree` of a worktree already gone succeeds; of one whose directory vanished, prunes its registration; **of one whose parent cannot be searched is refused and keeps it** |
| Rust unit | `tddy-worktree-service/src/worktrees.rs` | **a forced removal of a worktree that cannot be looked at is refused; a forced removal leaves it neither on disk nor listed** |
| Rust unit | `closes_in_flight.rs`, `peer_session_answer.rs` | **a session held by a close refuses other work until it ends; only the not-held-here refusal reads as "gone"** |
| Daemon integration | `tddy-session-lifecycle/tests/close_session.rs` | preview of clean / dirty / committed-not-pushed; unconfirmed dirty close does nothing; confirmed close ends main + tmux tab and removes worktrees (the session's and a conversation's, with its branch and base ref), keeps record and branch; fingerprint drift refused; a surviving tab skips removal; `holds_worktree` false after close; two sessions close concurrently; a second close of a closing session is refused; **while a close runs, resume, a new tab and delete are refused**; **an agent that writes as it is ended makes the re-check ask, nothing removed**; **a cancelled close skips every removal**; **an unreadable `tabs.yaml` fails the check (close) and refuses the delete**; **an unreadable conversation-worktrees directory, and a session worktree that cannot be looked at, are unreadable, not gone**; **codebase half** on this daemon (listed, closed, gone when not held, unreadable record) and on a scripted peer (previewed and closed there confirming its fingerprint, "not held here" is gone, any other refusal is unreadable, unreachable peer); **agent clones** from the record (torn down by a close and forgotten from the record, a closed session then deletes with the room down, an unreachable clone fails its removal, a delete tears down a record-only clone); **a remote conversation** cancelled by a close, and one that cannot be cancelled fails the close and refuses the delete |
| Daemon integration | `tddy-session-lifecycle/tests/close_target_session.rs` | target worktree preview and removal over the stand-in target; unreachable target → `unavailable_reason`, nothing done; delete of a target session whose worktree is already gone succeeds; **a closed tmux-run target session deletes once its target is unreachable**; **a target worktree whose parent cannot be searched is unreadable, not gone** |
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

- `svc_close_session/close_task.rs` is at **601** production lines after the validation fixes
  (budget 500). Deferral consented by the developer on 2026-11-24; the `move_impl_members` plan of the
  terminate phases into `close_task/terminate.rs` already passes `check --deep`
  ([`2026-11-24-close-task-rs-over-budget.md`](../todo/2026-11-24-close-task-rs-over-budget.md)).

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

## Validation Results

**Last run:** 2026-11-24 — `/validate-changes`, `/validate-tests`, `/validate-prod-ready`,
`/analyze-clean-code` (file length handled separately), scope `origin/master...HEAD` at 837a3590a;
fixes applied on top of 4d17b281e.

**Status:** ✅ the blocker and the 9 should-fix items are fixed (W9 by the restructure engine);
nits N1–N5, N11 and N12 done (N4 partly, see below); N6–N10 left as recorded choices.

**Build (scoped):** `./dev cargo clippy -p tddy-cli-sessions -p tddy-worktree-service
-p tddy-session-agents -p tddy-agent-launch -p tddy-session-lifecycle --all-targets -- -D warnings`
clean; `rustfmt --check` clean on every changed `.rs` file.

**Tests (scoped, local):**
- `tddy-session-lifecycle`: `close_session` 62/62, `close_target_session` 7/7, `delete_session` 8/8.
  The 12 fix tests were run red against the pre-fix code first (W1, W2 ×2, W3 ×2, W4 ×3, W5, W6,
  W7 ×2); the W8 coverage tests passed against it, as they cover existing behaviour.
- `tddy-worktree-service`: lib 34/34, `worktree_loss` and `target_worktree_actions_acceptance` green;
  only the 12 known `remote_git_livekit_acceptance` failures (environmental).
- `tddy-session-agents`, `tddy-cli-sessions`: all green.
- `tddy-agent-launch`: green except `clone_askpass_acceptance` (9), which failed only because
  `tddy-askpass` was not built in this worktree — **left to CI**.
- Full `tddy-session-lifecycle` package: stopped part-way on the developer's instruction (488 passed,
  12 failed, every failure in the known environmental baseline) — **left to CI**.
- No web or proto surface changed, so no Cypress spec was run.

**Fixes:**
- ✅ B1 rustfmt of the five lifecycle files.
- ✅ W1 a close holds its session: `ResumeSession`, `StartTerminalSession`, `DeleteSession` refused
  `ALREADY_EXISTS` while it runs (`SessionClosesInFlight` port; terminal roster `start`).
- ✅ W2 unreadable `tabs.yaml`: CHECK fails (close), delete refused; no file is no tab.
- ✅ W3 a torn-down clone is removed from `.session.yaml agents[]` in `tear_down_agent_clone`.
- ✅ W4 only `FAILED_PRECONDITION` carrying `SESSION_NOT_HELD_HERE` reads as "gone"; an unreadable
  record is `INTERNAL`.
- ✅ W5 a closed (marker) target session's main and tab tmux sessions are not asked of the target on
  delete.
- ✅ W6 conversation-directory listing and recorded clones propagate errors (only `NotFound` is none).
- ✅ W7 presence read with its error, local and target; planned presence carried into removal; both
  removals verified afterwards.
- ✅ W8 tests for the re-check, codebase forwarding (here and over a scripted peer), clone teardown,
  conversation ending (remote) and cancellation, plus one per fix.
- ✅ W9 `terminate_every_tab`, `codebase_worktrees`, `conversation_worktrees_in`, `delete_session_as`
  split by `tddy-tools restructure` `extract_method` (each plan `check --deep` clean, applied with the
  compile gate green; no refusals). Hand edits: doc comments, one `let_and_return`.
- ✅ N1 progress note; ✅ N2 `tmux_session_lives`; ✅ N3 `ClosedTurnLoop`; ⚠️ N4 partial — `stop()`
  answers nothing, so only a re-registered workspace jail is caught; ✅ N5 nested removal failure
  keeps the enclosing worktree; ✅ N11 Packages line; ✅ N12 PRD paragraphs wrapped.
- ✅ The progress channel id `"0"` is named (`PROGRESS_CHANNEL_ID`); `project_has_ref` moved into
  the support module.

**File length:** `close_task.rs` grew to 601 production lines — deferred with developer consent (see
Restructuring).

**Overall:** 8/10.

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
- [ ] Subagent-conversation coverage of a close covers a **remote** conversation only
  (`test_util::open_a_remote_conversation`): a local one needs a `SubagentSession`, opened only through
  `tddy-daemon-rpc`'s harness. Add a local close case there (`session_agent_conversation_acceptance.rs`),
  including an interrupted turn that does not stop (`ClosedTurnLoop::turn_still_runs`).
- [ ] `DeleteSession` of a closed target session skips each tab's tmux on the target as well as the
  main's (`SessionTab::without_its_target_tmux`); only the main's is covered
  (`close_target_session.rs`). A target tmux *tab* case needs the `remote_target_tmux_tabs` harness.
- [ ] Web: `closeSessionPage.revealEndedRow` copies the More-fold logic of
  `sessionsDrawerPage.selectEndedSession`; extract one shared "open the fold this row is under"
  helper and have both use it.
- [ ] Web: `aSessionServiceBackend`'s builders still take `Partial<…Init>` overrides, which `tsc`
  rejects (pre-existing errors at `aSessionEntry` / `aProjectEntry`); `closeSessionBackend.ts`
  avoids the pattern, and the rest of the file should follow it.
