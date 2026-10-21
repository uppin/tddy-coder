# Initial discovery: the `#satellite` sweep (node 16/16)

**Date:** 2026-10-21
**Base:** `origin/master` `bcdbb08af` (every `#satellite` node, #623–#635, #689, #690, is merged)
**Pre-stack base for every "before" figure:** `fb9879af8` (the parent of `97d0ec7d4`, `#satellite` 1/13)

This file is the evidence the changeset ([2026-10-21-satellite-sweep.md](./2026-10-21-satellite-sweep.md))
is distilled from: how the backlog was collected, every measurement, the seam reading per file and
what the engine is expected to refuse.

## 1. Collect — what the stack's own commits touched

The stack is the fifteen squash commits whose subject carries `#satellite` (`git log
--grep='#satellite' fb9879af8..origin/master`): #623–#635, #689, #690. 47 commits landed on `master`
over the same range; the other 32 belong to other stacks (`#reshape`, `#host-desktops`,
`#provider-route`, the clone-passphrase work, the code-index work) and are **not** attributed here.

### 1a. Records the stack's commits added, modified or deleted

`git diff-tree` per `#satellite` commit over `docs/dev/todo/` and `packages/*/docs/code-issues/`.

**Code-issue records added by the stack** (not in the records at `fb9879af8`):

| Record | Added by | File now |
|---|---|---|
| `packages/tddy-terminal-rpc/docs/code-issues/oversized-file-pty-relay.md` | #623 (n1) | 784 |
| `packages/tddy-host-service/docs/code-issues/oversized-file-host-registry.md` | #627 (n5) | 596 |
| `packages/tddy-agent-launch/docs/code-issues/oversized-file-session-coordinate-handlers.md` | #627 (n5) | deleted by #635 (n13): 483 — **closed** |
| `packages/tddy-session-sync/docs/code-issues/oversized-file-sync.md` | #629 (n7) | 788 |
| `packages/tddy-livekit/docs/code-issues/oversized-file-participant.md` | #630 (n8) | 907 |
| `packages/tddy-tools/docs/code-issues/oversized-file-cli.md` | #690 (n15) | 895 |

**Pre-existing records the stack modified** (regressed, narrowed or reworded): `oversized-file-runtime`,
`complexity-runtime-build` (tddy-daemon); `oversized-file-build` (tddy-service); `oversized-file-attach`
(tddy-session-sync); `oversized-file-lib` (tddy-session-tool-client);
`oversized-file-telegram-session-control-{mod,session-start}` (tddy-telegram-control);
`complexity-auth-build-auth-entries-admitting`, `oversized-file-auth` (tddy-daemon-auth);
`oversized-file-session-room`, `oversized-file-livekit-peer-discovery` (tddy-daemon-livekit);
`oversized-file-connection-service` (tddy-session-lifecycle); `oversized-file-config` (tddy-daemon-kernel,
wording only); `oversized-file-service` (tddy-host-service, tddy-worktree-service);
`oversized-file-project-coordinate-handlers` (tddy-daemon-rpc); `oversized-file-session-agent-clone`
(tddy-session-agents); `oversized-file-run` (tddy-coder); `oversized-file-screen-sharing-service`
(tddy-screen-sharing; since deleted by #653); `complexity-svc-resume-claude-cli-session-…`
(tddy-agent-launch, wording only); the three `tddy-session-split` complexity records (touched, unchanged);
`complexity-cli-run-call-tool` (tddy-tools, location only). **Deleted by the stack:**
`heavy-dependency-livekit-peer-forwarding` (tddy-daemon-kernel, #624).

**TODO entries the stack added:** `2026-10-17-satellite-1-left-files-over-the-500-production-line-budget`,
`2026-10-19-satellite-7-client-transports-left-files-over-the-500-line-budget`,
`2026-10-19-terminal-exposure-lock-spans-the-join`, `2026-10-21-session-delta-stores-adapter-lives-in-the-daemon`,
`2026-10-21-tddy-daemon-binary-cannot-be-built-without-the-livekit-plugin`,
`2026-10-21-restructure-cannot-move-a-binary-root-or-a-test-crate`,
`2026-10-21-restructure-cannot-reparent-modules-declared-in-a-binary-root`. It deleted
`2026-09-10-the-dependency-boundary-harness-is-duplicated-per-crate` and narrowed four older entries
(`2026-08-15-livekit-connect-streamer-duplication-still-outstanding`,
`2026-09-06-from-the-2026-09-06-optional-livekit-stack-evaluation`,
`2026-09-12-session-agent-clone-is-two-daemons-in-one-module`,
`2026-09-23-three-carved-crates-dev-depend-back-on-tddy-core`) — those are pre-existing and not the
stack's to sweep.

The three LiveKit testkit TODOs of 2026-10-18 (`…-livekit-docker-testkit-can-stall-the-event-loop-with-no-deadline`,
`…-livekit-web-start-echo-server-intermittently-exceeds-its-task-timeout`,
`…-web-tests-start-livekit-from-the-floating-master-image`) were added by **#651** (`ci(web): print
LiveKit testkit phases…`), which is not a `#satellite` commit. They are classified for the TODO gate
because the brief asked, and marked as not this stack's.

### 1b. Production lines of every non-test Rust file the stack touched

Metric: lines before the first `#[cfg(...test...)]` (`/pr-wrap` step 3.5's awk,
`/^[[:space:]]*#\[cfg\(.*test[),]/ {exit} {n++}`). "Stack Δ" is the sum of each `#satellite`
commit's own before→after on that file (`git show <c>^:<f>` vs `git show <c>:<f>`), so another stack's
growth over the same range is not counted. Files at or over 450 lines now:

| File | `fb9879af8` → `master` | Stack Δ | Nodes | Record |
|---|---|---:|---|---|
| `tddy-daemon/src/runtime.rs` | 1,781 → 1,867 | **+80** | 1,2,3,4,5,6,10,12,13 | `oversized-file-runtime` |
| ↳ `runtime::build` (brace matching) | 1,010 → 1,080 | grown | 1,4,6,12,13 | `complexity-runtime-build` |
| `tddy-service/build.rs` | 723 → 743 | **+14** | 1 | `oversized-file-build` |
| `tddy-session-sync/src/attach.rs` | 520 → 555 | **+35** | 1,7 | `oversized-file-attach` |
| `tddy-session-sync/src/sync.rs` | 788 → 788 | 0 | 7 | `oversized-file-sync` (**new, n7**) |
| `tddy-session-tool-client/src/lib.rs` | 1,111 → 1,114 (true count; the awk read 610 at the base because of a since-removed `cfg(any(feature = "livekit", test))` false stop) | **+3** | 7 | `oversized-file-lib` |
| `tddy-terminal-rpc/src/pty_relay.rs` | 775 → 784 | **+9** | 1,7 | `oversized-file-pty-relay` (**new, n1**) |
| `tddy-telegram-control/src/telegram_session_control/mod.rs` | 539 → 557 | **+18** | 3,6 | `oversized-file-telegram-session-control-mod` |
| `tddy-telegram-control/src/telegram_session_control/session_start.rs` | 713 → 719 | **+6** | 6 | `oversized-file-telegram-session-control-session-start` |
| `tddy-tools/src/cli.rs` | 895 → 895 | 0 | 15 (re-parented binary → library) | `oversized-file-cli` (**new, n15**) |
| `tddy-livekit/src/participant.rs` | 956 → 907 | −49 | 8 | `oversized-file-participant` (**new, n8**) |
| `tddy-host-service/src/host_registry.rs` | 596 → 596 | 0 | 5 | `oversized-file-host-registry` (**new, n5**) |
| `tddy-session-activity/src/service.rs` | 819 → 835 | **+16** | 13 (`NoSessionDeltaStores`) | **none — a finding no record carries** |
| `tddy-daemon-auth/src/auth.rs` `build_auth_entries_admitting` | 110 → 111 lines, 5 → **6** parameters | +1 / +1 param | 4 (`minter`) | `complexity-auth-build-auth-entries-admitting` |
| `tddy-daemon-livekit/src/session_room.rs` | 2,874 → 2,652 | −222 | 1,11,12,13 | `oversized-file-session-room` |
| `tddy-daemon-livekit/src/livekit_peer_discovery.rs` | 1,658 → 1,390 | −268 | 1,2,10,13 | `oversized-file-livekit-peer-discovery` |
| `tddy-session-lifecycle/src/connection_service.rs` | 527 → 516 (whole file: every `#[cfg(test)]` in it is a one-line attribute, the awk reads 18) | −15 (+5,+1,+4,−1,+5,−29) | 1,5,6,11,12,13 | `oversized-file-connection-service` |
| `tddy-coder/src/run.rs` | 2,678 → 2,655 | −23 | 8,15 | `oversized-file-run` |
| `tddy-host-service/src/service.rs` | 937 → 972 | −1 | 2 | `oversized-file-service` (claimed in practice by #693) |
| `tddy-session-agents/src/session_agent_clone.rs` | 1,157 → 1,156 | −1 | 11 | `oversized-file-session-agent-clone` |
| `tddy-daemon-rpc/src/project/coordinate_handlers.rs` | 658 → 652 | −8 | 5,10 | `oversized-file-project-coordinate-handlers` |
| `tddy-daemon-auth/src/auth.rs` (file) | 622 → 619 | −3 | 4,10 | `oversized-file-auth` |
| `tddy-worktree-service/src/service.rs` | 777 → 772 | −5 | 3 | `oversized-file-service` |
| `tddy-session-files/src/service.rs` | 553 → 553 | 0 | 10 | none (see § 1d) |
| `tddy-telegram-control/…/chaining_and_listing.rs` | 518 → 518 | 0 | 3 | `oversized-file-telegram-session-control-chaining-and-listing` |
| `tddy-screen-sharing/src/screen_sharing_service.rs` | 967 → 482 | +16 (n4), split by #653 | 4 | deleted by #653 — closed |
| `tddy-agent-launch/src/session_coordinate_handlers.rs` | 452 → 483 | +31 | 1,5,13 | deleted by #635 — closed, under 500 |

Files the brief named that are within budget: `tddy-testing-commons/src/dependency_graph.rs` 0 → 165
(created by n1, grown by n14) and `tddy-daemon-extensions/src/lib.rs` 0 → 50 (created by n1). Neither
needs work.

### 1c. Function-length / parameter records the stack touched

| Record | Before → after | Verdict |
|---|---|---|
| `complexity-runtime-build` | `build` 1,010 → 1,080 lines | grown by the stack → in the backlog (closes with `runtime.rs`) |
| `complexity-auth-build-auth-entries-admitting` | 110 → 111 lines, 5 → 6 params | grown by the stack → in the backlog |
| `complexity-cli-run-call-tool` | 93 → 93 | location only; out (moves with its module, Location updated at wrap) |
| `complexity-svc-resume-claude-cli-session-resume-claude-cli-session` | 119 → 119 | wording only; out |
| `complexity-svc-spawn-split-agent-spawn-split-agent` (118), `complexity-split-claude-cli-start-start-split-claude-cli-session` (149), `complexity-svc-paired-codebase-teardown-delete-paired-codebase-session` (96) | unchanged | out |
| `complexity-participant-run` (tddy-livekit, `run` 198) | not touched | out; moves with `participant::serve`, Location updated at wrap |
| `complexity-service-stream-acp-replay` (tddy-session-activity, 80) | not touched | out; moves with the trait impl, Location updated at wrap |

### 1d. `/analyze-code-issues` — what was run

- **Line-count / structure pass: run**, as the table in § 1b (the gate's metric over every non-test
  Rust file the stack's commits touched) plus brace-matched function lengths for the two complexity
  records the stack edited.
- **CRAP (coverage) pass: SKIPPED.** `tddy-tools analyze coverage` instruments and re-runs every suite
  of eleven packages; on this machine (load average 12–19 during this session) that is hours. No CRAP
  finding is claimed added or cleared by this sweep; `[W]` re-measures line counts, which is what every
  record in this backlog is about.
- **Finding with no record:** `tddy-session-activity/src/service.rs` (835) is over budget and has no
  `oversized-file-*` record; #635 grew it +16. It is in the backlog as a finding the stack grew.
- **Finding with no record, out of scope:** `tddy-session-files/src/service.rs` (553), touched by #632
  (n10) and unchanged. Alert-only by `/pr-wrap` 3.5's own table, so it should carry a record; the brief
  scopes it out (not grown). Listed for the developer, not planned.

## 2. Seam reading per file

Item spans are top-level items by brace matching, production part only.

### `tddy-session-sync/src/attach.rs` (555)

`DaemonHttpError` + `Display`/`Error` (371–407), `DaemonHttp` + its `impl` (414–532), `connect_error`
(536–555) — the daemon RPC client the record names. ~185 lines out → **~370**. `impl From<DaemonHttpError>
for AttachError` (170–174) and `open_session_room` stay and name the moved type through the facade.

### `tddy-session-sync/src/sync.rs` (788)

- git transport: `GitInvocation` (+impl), `GitTransport` (+impl), `remote_url`, `wip_ref`,
  `first_attach_commands`, `reconcile_commands` (82–211, ~130) and `run_git` (766–788).
- delta decisions: `RecordDecision`, `IgnoreReason` (+Display), `decide_record`, `WorktreeDecision`,
  `decide_worktree`, `delta_request`, `DeltaError` (+Display, Error), `reassemble` (215–419, ~205).

Both out → **~430**; `run` and `Syncer` stay.

### `tddy-session-tool-client/src/lib.rs` (1,114)

- room transport: `LiveKitRoomKey` … `dispatch_request_via_livekit` (434–729) and `livekit_session`
  (817–824), ~305.
- RPC transports: `NoCallbackToolService` … `dispatch_request_via_streaming_rpc` (734–1113 less 817–824),
  ~370.

Both out → **~440**. `conversation.rs` calls the `pub(crate) dispatch_request_via_*` — `reexport:
"outside"` re-points it; the crate's `pub` surface keeps resolving.

### `tddy-terminal-rpc/src/pty_relay.rs` (784)

- room path: `run_livekit_session`, `run_livekit_terminal`, `exchange_stub_session_token`,
  `join_terminal_room` (232–510, ~280) with the three constants (80–87).
- Connect-HTTP: `open_session_room_over_http`, `connectrpc_post`, `run_grpc_terminal`,
  `connectrpc_post_streaming` (518–765, ~245).

Both out → **~260**.

### `tddy-telegram-control/src/telegram_session_control/mod.rs` (557)

The seventeen request/outcome DTOs (57–201, ~145) into `types.rs` with a glob facade (every child reads
`use super::*`) → **~410**. The record's own plan.

### `tddy-telegram-control/src/telegram_session_control/session_start.rs` (719)

One inherent `impl` (6–719). `spawn_telegram_claude_cli` (217 lines) and `spawn_telegram_cursor_cli`
(137) out with `move_impl_members` into a sibling module → **~365**. This closes the file record; the
two functions stay over 60 lines, which no record carries and the record's own "extract along the
phases" (behaviour-risk, wants the spawns under test) is **not** planned.

### `tddy-tools/src/cli.rs` (895)

Four families, as the record proposes: workflow relay (`submit`, `ask`, `transition`,
`spawn-conversation`), session tools (`list-tools`, `call-tool`), schema and changeset
(`list-schemas`, `get-schema`, `persist-changeset-workflow`, `set-session-context`), session actions
(`list-actions`, `invoke-action`). `read_input`, `output_*`, `MAX_CLI_INPUT_BYTES` stay. Every family
is well under 500. ⚠ Each `relay_*` is a `cfg(unix)` / `cfg(not(unix))` **pair of same-named items**:
an `items` anchor names an item by path, so the pair may be ambiguous — `check --deep` decides.

### `tddy-livekit/src/participant.rs` (907)

- publisher: `SharedPublisher` (+impl), `spawn_response_drain`, `spawn_response_drain_reconnectable`
  (95–250, ~155), `move_item`.
- serve loop: `run`, `try_publish_codex_oauth_metadata`, `reassemble`, `handle_incoming` (483–812,
  contiguous members of the one `impl LiveKitParticipant<S>`), `move_impl_members`. ~330.

Both out → **~425**.

### `tddy-host-service/src/host_registry.rs` (596)

The sighting rules: `apply_sighting`, `apply_departure`, `validated_instance_id`, `is_instance_id_char`,
`bounded_label`, `bounded_repos_base_path`, `truncated_to` (316–452, ~137) → **~460**. `⚠` #693
(`#host-desktops` 10/10, draft) edits `tddy-host-service` but not this file (checked with `gh pr diff
693 --name-only`).

### `tddy-session-activity/src/service.rs` (835)

The port types (`OsUserResolver` … `ActivityPorts`, 60–190, ~130) and the frame helpers
(`activity_delta_frames`, `streamed`, `relayed`, 297–351) leave the file at ~645 — **not enough**: the
`#[async_trait] impl ActivityService for ActivityServiceImpl` is 481 lines by itself (354–834). The
engine refuses `extract_method` inside an `#[async_trait]` impl
(`docs/dev/todo/2026-10-19-restructure-extract-method-refused-under-async-trait.md`). A `move_item` of
the **whole** impl into `service/rpc.rs` would leave each file under 500 without cutting into it; whether
`move_item` accepts an `impl Trait for Type` anchor is what `check --deep` answers.

### `tddy-daemon-auth/src/auth.rs` — `build_auth_entries_admitting` (111 lines, 6 params)

`extract_method` candidates, bottom-up: the `livekit_token_entry` literal, the `user_resolver` closure,
the `credential_vaults` probe block (~22). Together ~111 → ~75: **not under 60**. The `auth_entry` match
holds `None => return Ok(AuthBuildResult::unauthenticated())`, an early `return` no `extract_method`
range may contain (`code-restructuring` rule). And 6 > 5 parameters is a signature change no engine
operation makes without a hand-written parameter type. Both need the developer's call (changeset
Decisions).

### `tddy-service/build.rs` (743)

`fn main` is 682 lines of independent `prost_build` / `tonic_build` chains (only `OUT_DIR` is shared).
The record's cure — one `compile_<family>()` per group, moved into a `build/` directory `#[path]`-included
from `build.rs` — is `extract_method` plus a module move **inside a build-script crate root**. The engine
resolves a package name to `src/lib.rs` only (`docs/dev/todo/2026-10-21-restructure-cannot-reparent-modules-declared-in-a-binary-root.md`),
so `build.rs`'s items have no address it can name. Expected to be refused; probed.

### `tddy-daemon/src/runtime.rs` (1,867) and `build` (1,080)

Outside `build`, the record's seams hold at today's lines:

| Seam | Items | Lines |
|---|---|---:|
| A — tasks | `DaemonChildren` … `impl RuntimeTasks` (209–504) | ~296 |
| B — env | `apply_env_overrides`, `env_var`, `data_dir_from`, `tddy_data_dir_for` (510–590) | ~81 |
| C — telegram + split rooms | `TelegramWiring`, `split_room_hosting`, `build_telegram` (1732–1866) | ~135 |
| D — first-login | `first_login_enrolment`, both `this_process_os_user` (605–646) | ~42 |

A–D out → ~1,313, of which `build` is 1,080. **The file closes only when `build` is cut.** `build`'s
top-level statement groups, from its own comments:

| Group | Lines | ~ |
|---|---|---:|
| prelude: data dir, web host/port, auth config, host registry, common-room peers, signing key, room-token minter, `auth_result`, advertised key | 653–752 | 100 |
| integrations: token entry, common room, config authenticator, shared managers, session rooms/admissions, integration board, reach, terminal exposure, split rooms, Telegram | 753–837 | 85 |
| relay idle, mutable slots, `tasks` | 838–888 | 50 |
| the authenticated branch `if let (Some(user_resolver), …)` | 889–1671 | 783 |
| config service, service names, common-room serve, embedded agent socket | 1672–1728 | 57 |

Inside the authenticated branch: discovery (894–943), the resolvers (944–1055), model registry /
notification bus / host prompts / host + worktree services (1056–1164), the session host (1165–1233),
task registry / admissions / LSP / index (1234–1286), the binary-host block (1287–1415), server
registration (1416–1550), VM (1551–1582), screen sharing (1583–1616), credential sync (1617–1671).

`build` has **no `return`** (its early exits are `?`), so `extract_method` ranges are legal anywhere.
The open question is what the extracted helpers take: most groups read six to twenty locals of
`build`, so a plain `extract_method` produces helpers with more than five parameters — a new
`complexity` finding per helper, which the sweep may not add. See the changeset's Decisions.

## 3. Engine limits met by this backlog

| Item | Limit | Source |
|---|---|---|
| `tddy-service/build.rs` | items of a build-script crate root have no address (package name → `src/lib.rs` only) | `2026-10-21-restructure-cannot-reparent-modules-declared-in-a-binary-root` |
| `tddy-session-activity/src/service.rs` | `extract_method` refused inside `#[async_trait]` impls; whole-impl `move_item` unproven | `2026-10-19-restructure-extract-method-refused-under-async-trait` |
| `tddy-daemon-auth` `build_auth_entries_admitting` | early `return` in the range; no parameter-object operation | `code-restructuring` SKILL rules |
| `tddy-daemon` `build` | extracted helpers' parameter counts | none: a design question |
| `tddy-tools/src/cli.rs` `relay_*` | same-named `cfg` pairs in one `items` anchor | unproven |

No item moves a binary root or a `tests/*.rs` suite, and no item re-parents a module declared in a
binary root: `cli.rs` is a library module since #690.
