# Changeset: Session worktree sync (`tddy-session-sync`)

**Date**: 2026-08-15
**Status**: 🚧 In Progress
**Type**: Feature

## Affected Packages

- **tddy-session-sync** (new): [README.md](../../packages/tddy-session-sync/README.md)
  - the client crate and binary — credentials, room attach, delta application, managed mirror
  - [changesets.md](../../packages/tddy-session-sync/docs/changesets.md) — changeset index entry
- **tddy-service**: [README.md](../../packages/tddy-service/README.md)
  - `connection.proto` — `AgentActivityRecord.head_commit` / `.activity_seq` / `.changed_paths`,
    `StreamAgentActivityDelta` + `DeltaScope`, `StreamReadWorktreeFile`, `AgentActivityDeltaChunk`,
    `WorktreeFileChunk`
  - `session_activity.rs` (new) — the `session.activity` broadcast topic constant, beside
    `worktree_activity.rs`
  - [changesets.md](../../packages/tddy-service/docs/changesets.md) — changeset index entry
- **tddy-core**: [README.md](../../packages/tddy-core/README.md)
  - `agent_activity.rs` — three new `#[serde(default)]` fields on `AgentActivityRecord`
  - [changesets.md](../../packages/tddy-core/docs/changesets.md) — changeset index entry
- **tddy-daemon**: [README.md](../../packages/tddy-daemon/README.md)
  - [session-room.md](../../packages/tddy-daemon/docs/session-room.md) — WIP tree per tick, the
    WIP ref, the bounded delta ring and its path scoping, the activity broadcast
  - `worktree_files.rs` / `connection_service.rs` — `StreamReadWorktreeFile`,
    `StreamAgentActivityDelta`
  - [changesets.md](../../packages/tddy-daemon/docs/changesets.md) — changeset index entry
- **tddy-coder**: [README.md](../../packages/tddy-coder/README.md)
  - `session_participant/mod.rs` — report `PresenterEvent::AgentActivity` to the daemon so
    coder-hosted sessions are not a blind spot
  - [changesets.md](../../packages/tddy-coder/docs/changesets.md) — changeset index entry

## Related Feature Documentation

- [PRD: Session worktree sync](../ft/daemon/session-worktree-sync.md)
- [Session rooms](../ft/daemon/session-room.md) — extended, not amended
- [Remote git repository over LiveKit](../ft/daemon/remote-git-repo.md) — reused verbatim

## Summary

Make a session's LiveKit room sufficient to **reconstruct** its worktree, not merely to learn that
it changed, and add a client that does so. The session-room poll loop gains a WIP tree per tick
(temporary index, `add -A`, `write-tree`), publishes it as `refs/tddy/session/{id}/wip`, and keeps a
bounded ring of `git diff --binary` patches between consecutive ticks. Agent activity records gain
the commit they ran upon, the tick their delta belongs to, and the paths they are credited with —
so a call's patch is scoped to its own files — and are broadcast into the session room. Two new
server-streaming RPCs let a participant pull a scoped delta by `call_id` and read a worktree file as
bytes. `tddy-session-sync` consumes all of it and maintains a fully managed local mirror, resyncing
by fetching the WIP ref whenever anything diverges.

## Where this stands

**Landed and merged-ready** (PR #401): the wire contract, and every primitive the wiring composes
from — each tested. 660 tests green, `clippy -D warnings` and `fmt --check` clean.

**Working end to end for daemon-hosted sessions.** The poll loop stages a WIP tree, produces a
scoped delta and publishes `refs/tddy/session/{id}/wip`; records are stamped with the commit they
ran upon and the paths they declared, and broadcast into the room; both RPCs serve; and
`tddy-session-sync` attaches, mirrors, and reconciles. Eight of the nine pieces are done.

**All nine pieces are done**, and the wiring is pinned by a LiveKit-backed suite that opens a real
room over a real server. One session type is still out of reach — see below.

| # | Remaining | Why it matters |
|---|---|---|
| ~~1~~ | ~~Wire `tick_delta` + `publish_wip_ref` into the poll loop~~ | ✅ done |
| ~~2~~ | ~~Wire `delete_wip_ref` into room close~~ | ✅ done, in `close` (not `Drop`), with a release interlock so a tick in flight cannot republish after deletion |
| ~~3~~ | ~~`session.activity` broadcast from the daemon~~ | ✅ done |
| ~~4~~ | ~~`tddy-coder` reports activity via `ReportAgentActivity`~~ | ✅ **solved without the transport** — the poll loop tails `agent-activity.jsonl`, which the coder already writes and the daemon already owns. No new coder credential, no new channel. |
| ~~5~~ | ~~Stamping at the three producers~~ | ✅ done |
| ~~6~~ | ~~`StreamAgentActivityDelta` handler + adapter~~ | ✅ done, serves all three scopes |
| ~~7~~ | ~~`StreamReadWorktreeFile` handler + adapter~~ | ✅ done |
| ~~8~~ | ~~Client attach + sync loop~~ | ✅ done, 31 new tests |
| ~~9~~ | ~~`./release`, `./install`, `./publish.sh` ship the binary~~ | ✅ done, pinned by `install_script.rs` |

**Audited 2026-08-29 against the code.** Most of the boxes below were stale rather than
outstanding: AC1, AC4 and AC20 all exist under different names (named inline where they are ticked),
and AC5 is **obsolete** — it would test the coder→daemon transport that piece #4 deliberately
replaced by tailing `agent-activity.jsonl`. What was genuinely missing was the package
documentation, now written, and the end-to-end suite AC31-36.

## Background

The session room broadcasts `WorktreeActivityEvent`, which deliberately carries no paths and no
content. `AgentActivityRecord` carries `tool_name` and the raw tool `input` but no commit, so a
consumer cannot know what state an edit applied to. `ReadWorktreeFile` returns `string
content_utf8`, hard-fails on any non-UTF-8 byte, and truncates at 1 MiB. Together these make a
mirror impossible to build correctly — not slow, *wrong*. See the PRD's "Why the existing signals
are not enough".

## Scope

- [x] **PRD Documentation**: [docs/ft/daemon/session-worktree-sync.md](../ft/daemon/session-worktree-sync.md)
- [x] **Changeset**: this document
- [x] **Package Documentation**: `packages/tddy-session-sync/docs/mirroring.md` (new), `packages/tddy-daemon/docs/session-room.md` § Reconstructing the checkout, and the client README's stale "not usable yet" status corrected
- [x] **Implementation**: proto additions, poll-loop WIP tree + delta ring, two RPCs, broadcast, client crate
- [ ] **Testing**: acceptance tests per package + one end-to-end suite over a real LiveKit server
- [x] **Integration**: cross-package — proto in tddy-service, producers in tddy-daemon/tddy-coder, consumer in tddy-session-sync
- [x] **Technical Debt**: `StreamReadWorktreeFile` duplicates `StreamReadHostDocument`'s SESSION_WORKTREE scope — recorded below as accepted, with the client's LiveKit secret
- [x] **Code Quality**: `clippy --workspace --all-targets -D warnings` + `fmt --check` clean

## Technical Changes

### State A (Current)

- `packages/tddy-daemon/src/session_room.rs:96-121` — `snapshot_worktree_within` measures
  `head_commit`, `branch`, numstat paths/counts and an untracked *count*. No content, no tree.
- `:143-169` — `activity_between` emits `COMMIT` and `FILES_CHANGED`; an untracked-only change
  emits nothing.
- `packages/tddy-core/src/agent_activity.rs:57-77` — `AgentActivityRecord` has nine fields, none
  of them a commit.
- Activity is served by `StreamSessionActivity` in the **common room**: by the coder participant
  `daemon-{inst}-{sid}` for tool/cursor sessions (`tddy-coder/src/session_participant/mod.rs:480-538`),
  by the daemon for claude-cli/sandbox (`tddy-daemon/src/connection_service.rs:8905-8998`). It is
  not broadcast anywhere.
- `ReportAgentActivity` (`connection_service.rs:8598`) is called only by `tddy-tools session-hook`
  (`tddy-tools/src/session_hook.rs:125`) — the claude-cli hook path. The coder never reports.
- `packages/tddy-daemon/src/worktree_files.rs:165` — `String::from_utf8(...)` →
  `FAILED_PRECONDITION` on binary; `:16` caps at 1 MiB and `:160` truncates on a raw byte boundary.
- No `StreamReadWorktreeFile` exists. The only binary worktree read is `StreamReadHostDocument`
  with `HOST_DOCUMENT_SCOPE_SESSION_WORKTREE` (`host_documents.rs:185-197`), addressed by
  `session_id` rather than `project_id` + `worktree_path`.

### State B (Target)

- `WorktreeSnapshot` gains `wip_tree: String`. `snapshot_worktree_within` writes it via a temporary
  index so the agent's own index is untouched.
- A `SessionDeltaStore` per hosted room: a bounded ring of `(seq, prev_seq, base_commit, patch)`
  plus a `call_id → (seq, declared paths)` index, evicting oldest-first by tick count and total
  bytes. A lookup narrows the tick's patch to the call's paths (`DeltaScope::Call`), to the paths no
  call claimed (`Residual`), or not at all (`Tick`).
- Each tick publishes `refs/tddy/session/{id}/wip` — `commit-tree <wip_tree> -p HEAD` — deleted when
  the room closes. **This replaces a cumulative-patch RPC entirely**: the mirror is a clone, so
  reconciling is `git fetch` + `git reset --hard`, which git does incrementally and
  delta-compressed.
- `AgentActivityRecord` gains `head_commit`, `activity_seq` and `changed_paths`, all
  `#[serde(default)]` on the Rust struct, fields 10/11/12 on the proto.
- The daemon broadcasts each record it knows about on `session.activity` in the session room.
- `tddy-coder` reports `PresenterEvent::AgentActivity` to the daemon via `ReportAgentActivity`, so
  the daemon is the single broadcaster for every session type.
- `StreamAgentActivityDelta` and `StreamReadWorktreeFile` on `ConnectionService`, served in the
  session room like the rest of it.
- New crate `tddy-session-sync` with a `[lib]` + `[[bin]]`, built by `./release`, installed by
  `./install`, packaged by `./publish.sh`.

### Delta

#### tddy-service
- **`proto/connection.proto`**: `AgentActivityRecord` += `head_commit` (10), `activity_seq` (11),
  `changed_paths` (12); new `AgentActivityDeltaRequest` / `AgentActivityDeltaChunk` / `DeltaScope`;
  new `WorktreeFileChunk`; two new server-streaming rpcs on `ConnectionService`. Both stream because
  neither payload has a useful upper bound, and an oversized LiveKit frame wedges a call silently
  rather than erroring.
- **`src/session_activity.rs`** (new): `SESSION_ACTIVITY_TOPIC = "session.activity"`, beside
  `worktree_activity.rs` and for the same stated reason — publisher and receiver live in different
  crates, so a topic each spelled for itself fails as silence.
- **`src/lib.rs`**: `agent_activity_to_proto` maps the three new fields.

#### tddy-core
- **`src/agent_activity.rs`**: `AgentActivityRecord` += `head_commit: String`, `activity_seq: u64`,
  `changed_paths: Vec<String>`, all `#[serde(default)]` so a JSONL written before this change still
  reads. Plus `declared_paths`, the closed set of tools that name a file they will write.
- **`src/git_head.rs`** (new): `read_head_commit`, **moved here from `tddy-daemon`**. Both sides of a
  session must stamp records with the same HEAD — the daemon for claude-cli and sandbox sessions,
  the coder's presenter for tool and cursor-cli ones — and `tddy-core` is the only crate both
  already depend on. It is pure filesystem work with no daemon concept in it, so the move costs
  nothing and `tddy-daemon` re-exports it.
- **`src/presenter/presenter_impl.rs`**: `set_agent_activity_context` gains the worktree, as an
  `Option` so a caller that does not know the checkout must *say so* rather than silently produce
  unstamped records. A terminal row inherits the commit its running row carried, so a call that
  commits does not restamp itself with the commit it just made.

#### tddy-daemon
- **`src/session_room.rs`**: `WorktreeSnapshot.wip_tree`; `write_wip_tree_within` (temporary index);
  `publish_wip_ref` / `delete_wip_ref` / `wip_ref_name`; `diff_between` with pathspec limiting and
  `changed_paths_between`; `SessionDeltaStore` + `DeltaScope` and its eviction; the poll loop records
  a tick delta, publishes the ref, and broadcasts activity records.
- **`src/connection_service.rs`**: `stream_agent_activity_delta` and `stream_read_worktree_file`
  handlers; `report_agent_activity` stamps `head_commit` and broadcasts.
- **`src/worktree_files.rs`**: `read_worktree_file_bytes` — the byte-exact reader the streaming RPC
  frames, sharing `validate_rel_path` and the git-listing gate with the UTF-8 one.
- **`src/connection_tonic_adapter.rs`**: the two new methods.

#### tddy-coder
- **`src/session_participant/mod.rs`**: on `PresenterEvent::AgentActivity`, POST
  `ReportAgentActivity` to the daemon in addition to the existing local append and live stream.

#### tddy-session-sync (new)
- `src/credentials.rs` — flags with per-parameter env fallback plus the repo-root `.env` reader;
  pure resolution over an injected map, as `tddy-remote-git-repo` does.
- `src/attach.rs` — resolve the session (project, worktree path, daemon instance) and join the room.
- `src/mirror.rs` — the managed destination: marker file, ownership refusal, clone, reset, apply.
- `src/apply.rs` — `git apply` of a delta, `seq` ordering and de-duplication.
- `src/reconcile.rs` — fetch, hard reset, cumulative delta, `StreamReadWorktreeFile` for the rest.

#### Root scripts
- `./release`, `./install`, `./publish.sh` learn `tddy-session-sync`.

## Implementation Milestones

- [x] Proto additions + codegen; `agent_activity_to_proto` mapping
- [x] `AgentActivityRecord` fields in tddy-core with backward-compatible deserialization
- [x] `wip_tree` in `WorktreeSnapshot` via a temporary index (seeded from the agent's, never written)
- [x] `publish_wip_ref` / `delete_wip_ref`
- [x] wire them to the poll tick and to room close
- [x] `diff_between` pathspec limiting + `changed_paths_between`
- [x] `SessionDeltaStore` with bounded eviction, path scoping, and the `call_id → (seq, paths)` index
- [x] `read_head_commit` (filesystem, no subprocess) + `declared_paths`
- [x] `tick_delta`
- [x] wire `tick_delta` + `publish_wip_ref` into the poll loop beside the activity broadcast
- [x] `head_commit` / `changed_paths` stamping in the coder's presenter
- [x] `head_commit` / `changed_paths` stamping at the two daemon producers
- [x] `session.activity` broadcast from the daemon
- [x] ~~`tddy-coder` reports activity to the daemon~~ — superseded: the poll loop tails `agent-activity.jsonl`, so no coder-side transport exists to build
- [x] `StreamAgentActivityDelta` handler + tonic adapter
- [x] `read_worktree_file_bytes`
- [x] `StreamReadWorktreeFile` handler + tonic adapter
- [x] `tddy-session-sync` credentials: flag/env/`.env` resolution, refusals
- [x] `tddy-session-sync` mirror: ownership, seq de-duplication, apply, reconcile reasons
- [x] `tddy-session-sync` attach: resolve the session, join the room, run the sync loop
- [x] Root scripts ship the new binary
- [x] `clippy --workspace --all-targets -D warnings` + `fmt --check` clean

## Testing Plan

### Testing Strategy

**Primary level: acceptance per package**, because each half of this is independently wrong-able.
The daemon's delta production is testable against a real git repo in a temp dir with no LiveKit at
all; the client's mirror management is testable against a local git remote with no daemon at all.
Only the broadcast wiring genuinely needs a room.

**Secondary level: one end-to-end suite** over a real LiveKit server, following
`remote_git_livekit_acceptance.rs` — real git, real binary, real room — to pin AC30-35.

**Deliberately not unit tests**: the delta is `git diff --binary` output and the application is
`git apply`. Asserting on patch *text* would pin git's formatting rather than the behaviour; every
assertion is on the resulting worktree bytes.

### Option 1: daemon delta production (primary)

**Test level**: Integration
**Location**: `packages/tddy-daemon/tests/session_activity_delta_acceptance.rs` (new)

**Scope**: WIP tree written without touching the agent's index; a tick delta that carries a new
untracked file, a deletion, a rename, a mode change and binary content; the cumulative delta; ring
eviction; `call_id → seq` lookup; the two distinct `NOT_FOUND` cases.

**Reliability**: deterministic — a temp git repo, ticks driven explicitly rather than by a timer.

### Option 2: streaming worktree read (primary)

**Test level**: Integration
**Location**: `packages/tddy-daemon/tests/stream_read_worktree_file_acceptance.rs` (new)

**Scope**: byte-exact round trip of a PNG and of a lone `0x80`; a file over 1 MiB not truncated; a
file over `max_attachment_bytes` refused before the first frame; an empty file yielding one empty
frame; frame size bound; the four guard cases (traversal, unlisted, symlink escape, missing).

### Option 3: client mirror management (primary)

**Test level**: Integration
**Location**: `packages/tddy-session-sync/tests/mirror_acceptance.rs` (new)

**Scope**: marker written on first attach; a non-empty unmarked dest refused; a dest marked for
another session refused; deltas applied in `seq` order; a repeated `seq` applied once; a `seq` gap
triggering reconcile; a rejected patch triggering reconcile; every divergence reported.

**Reliability**: deterministic — a local git repo as the remote, deltas supplied directly. No
LiveKit, no daemon.

### Option 4: end to end (secondary)

**Test level**: Production
**Location**: `packages/tddy-session-sync/tests/session_sync_livekit_acceptance.rs` (new)

**Scope**: AC30-35 against a real LiveKit server and a real daemon — Write, Edit, delete, binary,
commit, and recovery from a hand-corrupted mirror.

**Reliability**: `#[serial]`, multi-thread runtime, reusing `tddy_livekit_testkit::LiveKitTestkit`
as `remote_git_livekit_acceptance.rs` does.

### Coverage Requirements

- [ ] **Happy path**: Write, Edit, delete, binary and commit all reach the mirror
- [ ] **Error scenarios**: unknown `call_id`, aged-out delta, rejected patch, refused dest, oversized file
- [x] **Edge cases**: untracked-only change, rename, mode change, empty file, zero-length delta
- [x] **Integration points**: daemon reads the activity log; daemon → room broadcast; room → client. (No coder → daemon report exists — see piece #4.)
- [x] **Regression**: existing `agent-activity.jsonl` files still deserialize; `worktree.activity`
      events unchanged; `ReadWorktreeFile` behaviour unchanged

## Acceptance Tests

### tddy-daemon
- [x] **Integration**: `writes_a_wip_tree_without_touching_the_agents_own_index` (AC — WIP tree)
- [x] **Integration**: `a_tick_delta_carries_a_newly_written_untracked_file` (AC31)
- [x] **Integration**: `a_tick_delta_carries_a_deletion` (AC33)
- [x] **Integration**: `a_tick_delta_carries_binary_content_byte_for_byte` (AC34)
- [x] **Integration**: `publishes_the_uncommitted_state_as_a_ref_an_ordinary_git_fetch_can_reach` (AC13)
- [x] **Integration**: `parents_the_published_commit_on_the_head_it_was_taken_from` (AC13)
- [x] **Integration**: `keeps_the_wip_ref_out_of_the_branch_listing_an_agent_sees` (AC13)
- [x] **Integration**: `drops_the_wip_ref_when_the_session_ends_so_its_objects_stop_being_pinned` (AC13)
- [x] **Integration**: `scopes_a_calls_delta_to_the_files_that_call_touched` (AC6)
- [x] **Integration**: `serves_a_change_no_call_declared_as_the_ticks_residual` (AC7)
- [x] **Integration**: `every_call_scope_plus_the_residual_reconstructs_the_whole_tick` (AC7)
- [x] **Integration**: `gives_a_call_that_declared_nothing_an_empty_delta_rather_than_its_neighbours_changes` (AC8)
- [x] **Integration**: `evicts_the_oldest_delta_once_the_ring_is_full` (AC9)
- [x] **Integration**: `distinguishes_an_unknown_call_from_a_delta_that_aged_out` (AC8)
- [x] **Integration**: `reads_the_commit_a_checkout_is_on` (AC1)
- [x] **Integration**: `reads_a_detached_head` (AC1)
- [x] **Integration**: `reads_a_head_only_packed_refs_still_knows` (AC1)
- [x] **Integration**: `reads_the_head_of_a_linked_git_worktree` (AC1)
- [x] **Integration**: `reads_the_head_of_a_linked_worktree_whose_ref_is_packed_in_the_common_dir` (AC1)
- [x] **Integration**: `reports_an_unborn_branch_as_no_commit_rather_than_inventing_one` (AC1)
- [x] **Integration**: `agrees_with_the_snapshot_the_poll_loop_takes_of_the_same_checkout` (AC1)
- [x] **Integration**: `produces_a_delta_when_the_working_tree_moved` (tick wiring)
- [x] **Integration**: `produces_no_delta_when_the_working_tree_did_not_move` (tick wiring)
- [x] **Integration**: `produces_no_delta_on_the_first_tick_because_there_is_nothing_to_diff_from` (tick wiring)
- [x] **Integration**: `bases_a_delta_on_the_commit_the_tick_ended_at_when_the_agent_committed` (tick wiring)
- [x] **Integration**: `stamps_a_recorded_call_with_the_worktrees_current_head_commit` (AC1) — `agent_activity_stamping_acceptance.rs`
- [x] **Integration**: `a_recorded_call_is_broadcast_into_the_room_stamped_with_the_tick_it_ran_in` (AC4) — `session_room_livekit_acceptance.rs`
- [x] **Integration**: `reads_a_png_byte_for_byte` (AC13)
- [x] **Integration**: `reads_a_file_larger_than_the_unary_readers_one_megabyte_cap_without_truncating_it` (AC14)
- [x] **Integration**: `refuses_a_file_over_the_cap_rather_than_shortening_it` (AC14)
- [x] **Integration**: `reads_an_empty_file_as_zero_bytes` (AC15)
- [x] **Integration**: `refuses_a_symlink_that_resolves_outside_the_worktree` (AC17)

### tddy-core
- [x] **Unit**: `reads_an_activity_row_written_before_it_carried_a_commit` (AC3)
- [x] **Unit**: `credits_a_writing_tool_with_the_file_it_named` (AC2)
- [x] **Unit**: `credits_a_tool_that_declared_no_write_with_nothing` (AC2)
- [x] **Unit**: `returns_a_path_relative_to_the_worktree_because_that_is_what_a_pathspec_speaks` (AC2)
- [x] **Unit**: `drops_a_declared_path_that_falls_outside_the_worktree` (AC2)
- [x] **Unit**: `credits_nothing_when_the_input_names_no_usable_file` (AC2)

### tddy-coder
- [x] ~~**Integration**: `reports_its_own_activity_records_to_the_daemon` (AC5)~~ — **obsolete**: this would test the coder→daemon transport that piece #4 deliberately did not build. The daemon reads `agent-activity.jsonl` instead, covered by `session_activity_attribution_acceptance.rs`

### tddy-session-sync
- [x] **Integration**: `refuses_a_destination_it_does_not_own` (AC23)
- [x] **Integration**: `refuses_a_destination_marked_for_another_session` (AC23)
- [x] **Integration**: `applies_one_delta_once_when_several_calls_share_a_tick` (AC26)
- [x] **Integration**: `reconciles_when_a_sequence_number_is_missing` (AC27)
- [x] **Integration**: `reconciles_when_a_patch_does_not_apply` (AC28)
- [x] **Integration**: `names_the_expected_and_actual_values_of_every_divergence` (AC29)
- [x] **Integration**: `names_the_environment_variable_of_a_missing_credential` (AC21)
- [x] **Integration**: `never_prints_the_value_of_a_token_it_found_in_the_environment` (AC20) — `tddy-session-sync/tests/cli_acceptance.rs`
- [ ] **Production**: `mirrors_a_file_the_agent_wrote_without_a_commit` (AC31)
- [ ] **Production**: `mirrors_an_edit_to_an_existing_file` (AC32)
- [ ] **Production**: `removes_a_file_the_agent_deleted` (AC33)
- [ ] **Production**: `mirrors_binary_content_byte_for_byte` (AC34)
- [ ] **Production**: `follows_the_session_head_when_the_agent_commits` (AC35)
- [ ] **Production**: `restores_a_mirror_that_was_corrupted_by_hand` (AC36)

## Technical Debt & Production Readiness

- [~] **`StreamReadWorktreeFile` duplicates `StreamReadHostDocument`'s SESSION_WORKTREE scope.**
      Two RPCs read the same bytes through two resolvers. Accepted deliberately — the streaming
      sibling completes an obviously incomplete pair and lets the Code pane open an image — but the
      byte reader and the guards are shared (`read_worktree_file_bytes`, `validate_rel_path`, the
      git-listing gate) so the two cannot drift on *what* they allow, only on addressing.
      Collapsing them onto one reader is a follow-up.
- [~] **The client holds `LIVEKIT_API_SECRET`.** A real widening of the trust surface versus
      `tddy-remote-git-repo`, forced by `MintLiveKitToken` granting only the common room. Recorded
      in the PRD and in `docs/dev/TODO.md`; closing it means a mint that can grant a session room.
- [x] **Per-call scoping, with one limit named.** A call's delta is narrowed to the paths it
      declared, and the tick's residual carries what no call claimed. The limit that remains: two
      calls editing the *same* file in one poll window share that file's net change, because the
      measurement is per window. Named on the wire and in the PRD rather than papered over.
- [x] No fallbacks. Audited during PR wrap and one was found and removed: `diff_between` swallowed
      git failure into an empty patch, which — since the trees are known to differ — meant "this
      tick moved nothing", and a client would have recorded it applied and never reconciled. Both
      diff helpers are now `Result`, and `tick_delta` returns `None` rather than an empty patch for
      a failed diff or an unreadable `HEAD`. Reconcile remains a defined recovery path, not a
      fallback — it is reported every time it runs.

## Implementation Notes

- **Narrowing slices the recorded patch; it does not re-run `git diff` with a pathspec.** The PRD
  originally said the latter and has been corrected. Slicing is the better mechanism for a reason
  that only surfaced in implementation: **only the newest WIP commit is named by a ref**, so an
  older tick's trees are unreachable objects a `git gc` may reclaim, and re-diffing them would be a
  lookup that works until it silently does not. Slicing also gives the exact-partition property
  (every call's slice plus the residual is byte-for-byte the tick) and costs no subprocess.
  `diff_between` still uses pathspec limiting — that is how a tick's patch is produced, not how it
  is later partitioned.
- **The WIP index is seeded from a copy of the agent's index**, never the agent's own file. An empty
  scratch index would make `git add -A` re-hash the entire checkout on every poll tick.
- **`git commit-tree` runs under a fixed daemon identity**, because it refuses outright where no
  `user.email` is configured; parentless only on an unborn branch.
- **A latent traversal hole was closed on the way past.** `read_worktree_file_bytes` must answer
  `NotFound` before the listing gate (git cannot list a file that is not there), which exposed that
  `..\secret.txt` — one legal filename on Unix — was not treated as traversal and was only caught
  later as "unlisted". `validate_rel_path` now runs its traversal check on the same
  backslash-normalized path every later step uses. Consequence for the shipped unary RPC: a
  genuinely missing file is now `NOT_FOUND` rather than `PERMISSION_DENIED`.

- **`read_head_commit` answers for the root it is given and does not walk up to find a repository.**
  Every caller already holds the worktree root, and an upward search would silently answer for some
  enclosing repository instead of admitting it does not know — which for a mirror means stamping
  records with a commit from the wrong repo.
- **A ref name read off disk is not trusted.** `HEAD` and `packed-refs` are files, so the ref name
  they yield is validated (no `..`, no empty components, no absolute root, no backslash or NUL)
  before it is joined onto the git dir.

## Validation Results

Three independent read-only reviews (production-readiness, test quality, design/risk) plus the
toolchain gates. **Four defects were found and fixed; two of them were live on a shipped path.**

### Fixed — blockers

1. **A read-only measurement was writing to the repository.** `snapshot_worktree_within` called
   `write_wip_tree_by`, and its callers are the session-room poll loop (2 s default) *and* the
   `GetWorktreeSnapshot` RPC the web calls. So every hosted room copied the agent's index and ran
   `git add -A` + `write-tree` twice a second, materialising loose blob and tree objects in the
   project's **shared** object database — unreferenced from birth (nothing publishes the ref yet),
   with no `gc` and a two-week prune grace. It also put `wip_tree` into `WorktreeSnapshot`'s
   `PartialEq`, so a content-only edit defeated the poll loop's no-change short-circuit and forced
   a LiveKit metadata write carrying nothing new.
   **Fix:** the snapshot no longer measures a tree. Writing one is a side effect, not a reading, so
   it is done by whoever is about to *use* it and publish it as a ref in the same breath.
2. **Every path containing a space was silently unscopeable.** Git terminates a `---`/`+++` name
   with a literal tab exactly when it leaves a spaced name unquoted (`--- a/sp ace.txt<TAB>`);
   verified against git 2.43. `side_name` kept the tab, so the name matched no declared path and
   `DeltaScope::Call` returned an **empty** patch — indistinguishable from a call that declared
   nothing. **Fix:** strip the terminator (unambiguous, since git C-quotes any name containing a
   control character). Pinned by `scopes_a_calls_delta_to_a_file_whose_name_contains_a_space`,
   which was verified to fail without the fix.

### Fixed — a forbidden fallback

3. `diff_between` and `changed_paths_between` swallowed git failure into an empty result. Since
   `tick_delta` has already established the two trees **differ**, a failed diff produced a delta
   meaning "this tick moved nothing"; the client would record it applied, advance its sequence, and
   never reconcile the lost change. Both are now `Result`, and `tick_delta` returns `None` — never
   an empty patch — for a diff it could not take **and** for a `HEAD` it could not read (an empty
   `base_commit` matches no client HEAD and would reconcile forever while blaming a mismatch).

### Fixed — security

4. **An existence oracle for ignored files.** The gate asked "does it exist?" before "may you see
   it?", so probing `.env`, a private key or a build artifact returned `PERMISSION_DENIED` when
   present and `NOT_FOUND` when absent — keeping contents secret while handing out the existence
   map the listing gate exists to hide. **Fix:** the listing is consulted first; absence is only
   reported for a path git *does* list (a tracked file deleted from the worktree). The test that
   pinned the leaky behaviour was replaced by
   `refuses_an_unlisted_path_identically_whether_or_not_it_exists`.
5. `LIVEKIT_API_SECRET` was accepted as a **command-line flag**, landing in `/proc/<pid>/cmdline`
   for any local user. It is the key a daemon signs every session token with. The flag is gone;
   environment (or repo-root `.env`) only.
6. Documentation in `docs/dev/TODO.md` and the PRD's non-goals told a future implementer that
   `StreamReadWorktreeFile` is the way to reach build output and a local `.env` — which, if
   followed, would serve every session's `.env` over LiveKit. Corrected in both.

### Fixed — tests that could not fail

- `a_snapshot_reports_the_tree_it_measured` asserted `f(x) == f(x)` and passed on `"" == ""`.
  Replaced by one asserting git calls the result a tree and it holds the dirty file, plus
  `a_snapshot_does_not_write_a_tree_because_measuring_is_not_a_side_effect` (loose-object count
  unchanged), which pins blocker 1.
- The wiring suite's cross-check compared two values that are both `""` on total failure; it now
  anchors both against git.
- A core test asserted `42 == 42` from its own builder and named a behaviour it never invoked —
  removed.
- Six stale AC cross-references in code and test headers (the criteria were renumbered twice during
  planning) now match the PRD.

### Known and accepted

- `.env` read errors other than "absent" are now reported rather than silently contributing
  nothing; an empty environment value is treated as unset, matching what the module documented.
- `ReadWorktreeFile` on a *listed* file missing from disk returns `NOT_FOUND` where master returned
  `PERMISSION_DENIED`. No web consumer branches on the code (verified across `packages/tddy-web/src`).

### Gates

`build --workspace --all-targets` ✅ · `clippy --workspace --all-targets -D warnings` ✅ ·
`fmt --all --check` ✅ · tddy-daemon 585 ✅ · tddy-core ✅ · tddy-session-sync ✅ ·
tddy-coder ✅ · tddy-service ✅

## The wiring bug that isolated tests could not see

**`SessionDeltaStore::attribute()` had no production caller.** Every piece passed its own suite, and
the feature was dead: the store recorded deltas, nothing ever mapped a `call_id` to the tick it
landed in, so every `delta_for_call` answered `UnknownCall` — for `Call`, `Residual` *and* `Tick`,
since all three resolve the tick through that mapping. `activity_seq` stayed 0 forever.

**Only claude-cli sessions had a room at all.** `open_for` had one call site, so a cursor or tool
session had nothing to broadcast into and nothing to attach to.

Both are fixed, and the fix collapsed a second problem with them:

- **The poll loop is now the single broadcaster.** It tails the session's `agent-activity.jsonl`,
  attributes each new record to the tick's delta seq, stamps `activity_seq`, and publishes. One
  path serves every session type — claude-cli, sandbox, cursor and tool — which is also how a
  coder-hosted session's activity reaches the room **without** the daemon transport the coder does
  not have: the coder already writes that log, and the daemon already owns that directory.
- **The direct broadcast from `report_agent_activity` was removed.** It published a record *before*
  the tick holding its delta existed, so a client reacting to it looked the delta up and got
  `UnknownCall` — and once the poll loop broadcasts too, it would have duplicated every claude-cli
  record with `activity_seq: 0`.
- **A tick with new records but an unchanged tree records an empty delta.** Without it those calls
  resolve to `UnknownCall`, which a client reads as a defect rather than "this call changed
  nothing" (AC7).

**A LiveKit-backed suite now pins the wiring** (`session_room_livekit_acceptance.rs`, 6 tests): it
opens a real room over a real server, and the load-bearing one appends a record, dirties the file it
declares, waits a tick, and requires `delta_for_call` to **resolve** and its patch to reproduce the
edit. That is the assertion whose absence let this ship.

### The one session type still without a room

A **`tool`** session cannot get one, and this is a design constraint rather than an omission. At the
moment the daemon spawns it, the daemon hands `tddy-coder` the *project main repo path*; the coder
creates the session's worktree itself, later, from a branch suggestion inside its own workflow, and
writes the session directory itself. So at spawn time the worktree does not exist and its path is
not derivable — `SessionRoomRegistry::open` measures it, gets `Measurement::Gone`, and fails the
start. Opening the room against the main repo would pin the poll loop to the wrong checkout for the
session's whole life; opening it after the spawn would break the ordering the first-participant
guarantee rests on. Closing it means the coder reporting its worktree back to the daemon.

Rooms now open for **claude-cli, sandboxed claude-cli, cursor-cli and sandboxed cursor-cli**.
Sandboxed claude-cli was a second instance of the same bug, found while fixing the first:
`start_sandboxed_claude_cli_session` never reached the shared helper either.

## Races and invariants closed while wiring

- **A room closing mid-tick would have leaked its WIP ref forever.** `publish_wip_ref` runs inside a
  `spawn_blocking`; a tick already in that call when `close` ran would republish the ref *after* the
  deletion, and nothing would ever delete it again — the exact leak AC13 exists to prevent. Closed
  with a per-room release flag under a mutex held across the git call on both sides: a release waits
  for a publish in flight and then deletes what it wrote, and a tick that finds the room released
  publishes nothing.
- **Deletion belongs in `close`, not `SessionRoomTask::drop`.** `Drop` also fires when a room is
  replaced under the same id and when the registry is torn down at shutdown — neither means the
  session is over, and dropping every ref at shutdown would unpin every live session's uncommitted
  state. It runs **inline and synchronously**, because `DeleteSession` removes the session directory
  and `RemoveWorktree` the checkout immediately afterwards; a merely-scheduled deletion would race
  the removal it must precede.
- **Deltas have their own sequence space.** Sharing the activity `seq` would let event numbering (a
  commit plus a files-changed tick burns two) punch permanent holes in the delta sequence — and a
  client reads a delta gap as a lost broadcast and reconciles.
- **The previous WIP tree is kept beside the snapshot, not inside it.** The poll loop decides a tick
  is idle by comparing snapshots; folding the tree in would make every measurement differ, turning
  an idle room into one that stages the whole checkout and rewrites metadata at the poll rate.

## Corrections found while wiring

- **The PRD's reconcile sequence was wrong, and would have looped forever.** It said
  `git reset --hard refs/tddy/wip`, which parks the mirror's `HEAD` on the *WIP* commit — but every
  delta names the session's real `HEAD` as its `base_commit`, and the client compares that against
  `rev-parse HEAD`. Verified against real git: the documented form leaves `HEAD` on the WIP commit,
  so every following delta rejects, reconciles, parks there again, and reconciles forever. Corrected
  to `reset --hard <wip>^` (the WIP commit's parent **is** the session's commit) plus
  `read-tree -u --reset <wip>` to lay the uncommitted state over it without moving `HEAD`.
  AC27, AC28 and AC31 were all updated.
- **Session resolution cannot go over the room's RPC client.** It is circular: addressing that
  client needs `daemon-{instance_id}`, which is one of the things being resolved. It goes over
  Connect-HTTP to `--daemon-url` instead, which also means an unknown session fails *before* any
  room is joined or any directory taken over.
- **The mirror applies whole ticks, not per-call scopes.** `Mirror::apply` de-duplicates by tick
  `seq`, so applying per-call scopes would apply the first call of a poll window and return
  `AlreadyApplied` for every later call *and* for the residual — silently dropping both.
  `DELTA_SCOPE_TICK` is their union by definition. The per-call scope remains on the wire and
  tested; it is what tells a consumer *which call changed what*, and the tick is what gets applied.

## Decisions & Trade-offs

- **Deltas from the poll loop, not from tool inputs.** Synthesising a patch from `Edit`'s
  `old_string`/`new_string` is free and already on the wire, but blind to any writer that does not
  declare itself — a `Bash` call running a formatter, `sed -i`, a codegen step. A blind spot is a
  silently wrong mirror, which the feature exists to prevent. The cost is that attribution is
  per-window rather than per-call, which the wire states.
- **Both a WIP tree and a WIP ref, for two different jobs.** The tree is what tick deltas are
  diffed from — small, scoped, attributable, and what a client applies while it is keeping up. The
  ref is how a client that has *fallen behind* catches up, and it exists because the mirror is a
  clone: git already moves only the objects it is missing, delta-compressed, where a cumulative
  patch would re-send the whole dirty tree over a data channel every time. The cost is session-scoped
  refs in a shared project repository, which is why `delete_wip_ref` is part of room close rather
  than a later cleanup.
- **Scoping by declared path rather than by measured path.** A call is credited with what it said it
  would touch. Measuring which paths a *particular call* changed would mean diffing around every
  tool call, including read-only ones. The residual scope is what keeps the cheap choice from being
  lossy.
- **A temporary index, not the agent's.** `git add -A` against the real index would rewrite the
  agent's staging area mid-session. Non-negotiable.
- **Lazy delta fetch rather than inline bytes.** `AgentActivityRecord` also feeds the web Agent
  Activity pane and the on-disk JSONL, neither of which wants patch bytes; and an oversized LiveKit
  frame wedges a call silently rather than erroring.
- **The daemon is the sole broadcaster.** It is the only participant in the session room and the
  only thing running the poll loop that computes deltas. Making the coder report to it, rather than
  join the room to broadcast for itself, keeps one producer and one topic.
- **Reconcile over halt.** "Stop and report" is the strictest reading of "must not fail silently",
  but a mirror that stops on the first rejected patch is a mirror that stops. Reconciling from git
  and reporting at `error` is self-healing and still never quiet.

## References

- [PRD: Session worktree sync](../ft/daemon/session-worktree-sync.md)
- [Session rooms](../ft/daemon/session-room.md)
- [Remote git repository over LiveKit](../ft/daemon/remote-git-repo.md)
