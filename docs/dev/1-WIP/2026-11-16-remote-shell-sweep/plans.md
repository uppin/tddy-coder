# The sweep's plans: what each one moves, and what the engine refused

**Changeset**: [2026-11-16-remote-shell-sweep.md](../2026-11-16-remote-shell-sweep.md)
**Baseline**: [baseline.md](./baseline.md)
**Written**: 2026-11-16, on `7f6ed0b54` (`feature/remote-shell/sweep`), nothing applied.

Every anchor in the seven plan files was emitted by `tddy-tools restructure anchors` against this
checkout's warm index (`./run-index-daemon`), every header by `restructure snapshot`, and every plan
was proven with `restructure check <plan> --deep --budget 500`. No plan was applied, no `--dry-run`
was run, and no production or test file was edited.

| Plan | Group | Operations | `check --deep` |
|---|---|---|---|
| [`plan-A.jsonl`](./plan-A.jsonl) | A — `tddy-agent-launch` file split (row 1) | 2 | `no findings` |
| [`plan-B.jsonl`](./plan-B.jsonl) | B — `tddy-agent-launch` functions (rows 10, 11, 13, 14, 15) | 12 | `no findings` |
| [`plan-C.jsonl`](./plan-C.jsonl) | C — `tddy-session-lifecycle` (rows 2, 3, 4) | 5 | `no findings` |
| [`plan-D.jsonl`](./plan-D.jsonl) | D — `tddy-accounts` (row 5), first pass | 4 | `no findings` |
| [`plan-D2.jsonl`](./plan-D2.jsonl) | D — `tddy-accounts` (row 5), second pass | 1 | `no findings` |
| [`plan-E.jsonl`](./plan-E.jsonl) | E — `tddy-worktree-service` (row 6) | 3 | `no findings` |
| [`plan-F.jsonl`](./plan-F.jsonl) | F — `tddy-daemon` (row 12) | 2 | `no findings` |

Group D is two files, not one: see [Why D is two plans](#why-d-is-two-plans).

**What `check --deep` proved and what it did not.** It resolved every operation through the path
`apply` uses and reported no refusal. It does not compile anything: the compile gate, `clippy` and
`rustfmt` run only at apply. Every line count under "Projected" is arithmetic over the lines each
anchor covers (moved lines out, a `mod` line and the `use` lines a re-point adds in), not a
measurement; the tidy's removal of imports the old file no longer uses is not counted, so the
projections for the files that shrink err high. Function projections assume `rustfmt` puts a call
on one line when it fits in 100 columns and one argument per line when it does not.

## Three things the checker does that shaped these plans

They are behaviours of `check --deep` on this engine build, found while writing the plans. Each
cost at least one refused draft, quoted where it happened.

1. **A module a plan creates does not exist for the plan's later operations.** A `move_item` or
   `move_impl_members` whose `to` names a module an earlier line of the same plan creates is
   refused (`… does not exist: … declares no module …`). So every operation here carries `name`
   and fills its module in one contiguous run; no module receives two runs.
2. **An operation is refused when a place that names one of its items lies in text an earlier
   operation moved** (`anchor invalidated: position in … fell inside text removed by an earlier
   operation`). Definitions therefore move before their users, and two runs that name each other
   cannot both move in one plan.
3. **An operation whose run sits immediately above a run an earlier operation moved is refused
   the same way**; one immediately below is not. Adjacent runs are therefore taken top-down, and
   where rule 2 demands the opposite order the second run is a plan of its own (group D).

## Rows, targets and verdicts

"Before" is the changeset's State A value; "at the stack's base" is what the same counter reads on
the stack's merge-base `3b15ffbc4` (see `baseline.md`). Where they differ the stricter one is the
target used here.

| # | Row | Now | Before (changeset) | At `3b15ffbc4` | Target | Projected | Verdict |
|---|---|---|---|---|---|---|---|
| 1 | `remote_agent_launch.rs` | 579 | new | — | < 500 | ~340 | met (plan A) |
| 2 | `svc_activity_ports.rs` | 546 | 513 | 513 | < 500 | ~415 | met (plan C) |
| 3 | `svc_host_builders.rs` | 544 | 531 | 531 | < 500 | ~467 | met (plan C) |
| 4 | `connection_service.rs` | 567 total · 442 by `restructure lines` | "+6" | 558 total · 433 | ≤ 558 · ≤ 433 | 555 · 430 | met (plan C), by 3 lines |
| 5 | `tddy-accounts` `service.rs` | 790 | 772 | 772 | < 500 | ~425 | met (plans D + D2) |
| 6 | `remote_git_service.rs` | 1,111 | 1,004 | 1,004 | < 500 | ~425 | met (plan E) |
| 7 | `useCreateSessionPaneState.tsx` | 503 | 495 | 495 | < 500 | 498 | met (group H, by hand) |
| 8 | `gen/host_pb.ts` | 1,762 | 1,402 | 1,402 | unchanged | 1,762 | no seam (generated) |
| 9 | `start_cli_harness_session` | 368 | 319 | 318 | ≤ 318 | 368 | **not met** — engine refusals |
| 10 | `resume_cli_harness_session` | 166 | 157 | 136 | ≤ 136 | ~128 | met (plan B) |
| 11 | `resume_cli_harness` | 108 | 95 | 88 | ≤ 88 | ~84 | met (plan B) |
| 12 | `build_session_host` | 82 | 73 | 73 | ≤ 73 | 72 | met (plan F), needs one consented lint fix |
| 13 | `start_cli_harness_from_request` | 100 | 98 | 94 | ≤ 94 | ~85 | met (plan B) |
| 14 | `start_sandboxed_cli_harness_session` | 292 | 291 | 291 | ≤ 291 | ~287 | met (plan B) |
| 15 | `start_session_on_request` | 80 | 79 | 79 | ≤ 79 | 73 | met (plan B) |
| 16 | `DaemonSessionHost::new` | 100 | 97 | 97 | ≤ 97 | 100 | **not met** — engine refusals |

Row 15 is a function of `tddy-agent-launch` (`svc_start_session_core.rs`), not of
`tddy-session-lifecycle` as the changeset's Delta lists it, so its operations are in plan B.

## Rows that cannot be met with the engine

- **Row 9 — `start_cli_harness_session` (368, target ≤ 318).** Every seam that holds what the stack
  added passes or returns the session's target shell, whose type is `Option<Arc<dyn RemoteShell>>`.
  rust-analyzer writes `dyn RemoteShell` unqualified, `opencode_cli_spawn.rs` does not import
  `RemoteShell`, and the engine writes no `use`. Four ranges were refused for it and a fifth for
  `SeededAgentClones` (quoted under plan B). The ranges that do pass and stay at seven parameters
  or fewer sum to about 43 lines, short of the 50 needed, and are wrappers of code the stack did
  not write; none is in the plan. **What would unlock it** is two hand-written `use` lines in
  `opencode_cli_spawn.rs` (`tddy_remote_shell::RemoteShell`,
  `tddy_session_agents::seed_codebase::SeededAgentClones`) before the plan runs: the five refused
  ranges plus the hook-log block and the tail come to about 51 lines. That is a hand edit of
  production code ahead of a move, which needs the developer's consent, and it could not be proven
  here because nothing may be edited.
- **Row 16 — `DaemonSessionHost::new` (100, target ≤ 97).** The body is one `Self { … }` literal.
  Three field-expression ranges were refused (`rust-analyzer offers no "extract into function"
  assist`, quoted under plan C), exactly as the record says of the `#agent-tools` sweep. The one
  statement range that is offered returns a 7-tuple and shortens nothing.
- **Row 8 — `gen/host_pb.ts`.** Generated; no seam, as the changeset decides.

## Plan A — `remote_agent_launch.rs` becomes a module with `startup` and `target`

| # | Operation | What moves where |
|---|---|---|
| 1 | `move_item`, `name: startup`, `reexport: outside` | `AgentStart`, `wait_for_agent_start`, `exits_during_startup`, `settled_last_output`, `captured_bytes`, `last_output_of` (lines 440–578) → `remote_agent_launch/startup.rs` |
| 2 | `move_item`, `name: target`, `reexport: named` | `TargetError` with its `Display` and `Error` impls, `shell_for_target`, `target_config_host`, `session_target_of_proto`, `proto_of_session_target` (lines 34–141) → `remote_agent_launch/target.rs` |

**Projected production lines**: `remote_agent_launch.rs` 579 → ~340 (−139 and −108 moved, two `mod`
lines, four `use` lines for the startup names `launch_on_target` writes bare, the facade);
`startup.rs` ~150; `target.rs` ~118.

**Callers outside the crate**: none of the moved items. The three acceptance suites under
`tddy-agent-launch/tests/` import `launch_on_target`, `RemoteLaunch`, `RemoteLaunchError` and
`RemoteLaunched`, which stay.

**Why operation 2 is `named` and not `outside`.** The target items are named by path in eight places
of the crate, three of them inside functions plan B anchors (`opencode_cli_spawn.rs:108`,
`resume.rs:240`, `cli_branch_starts.rs:234`). `outside` would rewrite those lines to the longer
`remote_agent_launch::target::…` path: the functions' fingerprints change (plan B's anchors would
have to be re-emitted) and `cli_branch_starts.rs:234` passes 100 columns and wraps, growing row 13.
`named` leaves `pub use target::{…}` in `remote_agent_launch.rs` and touches no caller, so plans A
and B are independent and may be applied in either order.

**Visibility** (from `check --deep`): `AgentStart`, `wait_for_agent_start`, `exits_during_startup`,
`settled_last_output` private → `pub(super)`. Operation 2 widens nothing.

**Refusals**: none.

## Plan B — functions of `tddy-agent-launch`

| # | File · function | Operation | Range now | What it becomes |
|---|---|---|---|---|
| 1 | `resume.rs` · `resume_cli_harness_session` | `extract_method` `resumed_session_response` | 351–356 | the `Ok(Response::new(ResumeSessionResponse { … }))` tail |
| 2 | same | `extract_method` `record_the_resumed_session` | 337–347 | the metadata update and its write |
| 3 | same | `extract_method` `record_the_resumed_usage_context` | 297–306 | the `record_usage_context` call |
| 4 | same | `extract_method` `hooks_append_to_the_target_log` | 245–254 | the hook-log redirection the stack added |
| 5 | same | `extract_method` `remote_host_of` | 240–244 (the expression) | the checkout host: the target's alias, else the ssh exec host |
| 6 | `resume.rs` · `resume_cli_harness` | `extract_method` `resumed_worktree` | 95–99 (the expression) | where the agent resumes |
| 7 | same | `extract_method` `settle_a_tmux_agents_unwatched_exit` | 60–71 | the tmux settle |
| 8 | same | `extract_method` `resumed_split_wiring` | 46–59 | the split wiring re-derivation |
| 9 | `cli_branch_starts.rs` · `start_cli_harness_from_request` | `extract_method` `managed_wiring_of` | 166–182 | the managed recipe and its two spawn handlers |
| 10 | `svc_start_sandboxed_cli_harness_session.rs` · `start_sandboxed_cli_harness_session` | `extract_method` `jailed_agent_env` | 272–284 | the jail's environment map |
| 11 | `svc_start_session_core.rs` · `start_session_on_request` | `extract_method` `agent_def_for_start` | 81–88 | the agent-def lookup |
| 12 | `svc_start_session_core.rs` | `move_impl_members`, `name: started_session_name` | `record_the_started_sessions_name` | → `svc_start_session_core/started_session_name.rs` |

Operations in one function are last range first.

**Why operation 12 is here.** `svc_start_session_core.rs` is 499 production lines. Any
`extract_method` in it adds the new function's signature to the same file and takes it past 500,
which would open a record this PR may not add. Moving the 39-line
`record_the_started_sessions_name` out in the same plan leaves the file at about 468.

**Projected function lengths**: row 10 166 → ~128; row 11 108 → ~84; row 13 100 → ~85;
row 14 292 → ~287; row 15 80 → 73. Row 9 is not touched.

**Projected production lines**: `resume.rs` 387 → ~455; `cli_branch_starts.rs` 339 → ~347;
`svc_start_sandboxed_cli_harness_session.rs` 448 → ~466; `svc_start_session_core.rs` 499 → ~468;
`svc_start_session_core/started_session_name.rs` ~50; `opencode_cli_spawn.rs` 422, untouched.

**Callers outside the crate**: none; every new function is private to its file, and
`record_the_started_sessions_name` is a private method reached through `LaunchSessions`.

**Visibility**: `LaunchSessions::record_the_started_sessions_name` private → `pub(super)`.

**Refusals `check --deep` gave, verbatim, and what was tried next**

- Rows 9, 10, 11 — the target-shell seams. Ranges tried: `opencode_cli_spawn.rs` 109–118 (the
  target resolution), 291–302 (slot files and pointer through the resolved shell), 334–341 (the
  `HarnessRemote` expression); `resume.rs` 273–283 (the same slot-and-pointer block) and 75–86 (the
  resume's target resolution). Each:

  > this seam cannot be cut here: the extracted signature names `RemoteShell`, which this file does not import, and the function it came from spells it with no qualifier; import it or cut the range elsewhere

  Next for rows 10 and 11: seams that do not carry the shell (operations 1–8), which pass and
  reach the targets. Next for row 9: none reaches the target; see
  [Rows that cannot be met](#rows-that-cannot-be-met-with-the-engine).
- Row 9 — `opencode_cli_spawn.rs` 183–197 (the seeded-clone claim):

  > this seam cannot be cut here: the extracted signature names `SeededAgentClones`, which this file does not import, and the function it came from spells it with no qualifier; import it or cut the range elsewhere

- Row 9 — ranges that pass but were left out: 232–240 (the hook-log block, 8 lines; left out
  because operation 4 extracts the identical block in the child module `resume.rs`, and a second
  copy of the helper in the parent is a duplicate the engine cannot fold), 415–421 (the response
  tail), 123–125, 214–216, 259–267, 316–322, 383–390, 408–413. Two more pass with a note and are
  unusable: `takes 16 parameters; clippy::too_many_arguments fires above 7` (354–377, the metadata
  literal) and `takes 11 parameters; …` (137–152, the first changeset).
- Row 15 — `svc_start_session_core.rs` 89–98 (the allowed-agents guard run):

  > rust-analyzer's answer was unusable: rust-analyzer wrote the guard run extracted into a function in a shape the lift does not know: expected `if let Some(..) = refuse_an_unlisted_agent(..) { return ..; }` or `refuse_an_unlisted_agent(..)?;`

  Next: 81–88 (operation 11), which passes.

**What the compile gate may still say.** `check --deep` cannot see these: the type each new
signature spells for a conversation or a launched session, and the borrow of `wiring` across
operation 4. No extraction here ends in a bare `let x = …; x` (the two that would, operations 5
and 6, are anchored on the expression instead), so `clippy::let_and_return` should not fire.

## Plan C — `tddy-session-lifecycle`

| # | Operation | What moves where |
|---|---|---|
| 1 | `move_item`, `name: hook_delivery`, `outside` | `ActivityHookDelivery` and its `HookReportDelivery` impl (`svc_activity_ports.rs` 52–83) → `svc_activity_ports/hook_delivery.rs` |
| 2 | `move_item`, `name: orchestrator_presence`, `outside` | `ServedTerminalPresence`, its `OrchestratorPresence` impl and the second `impl DaemonSessionHost` (`peer_serves_main_terminal`, `ask_peer_whether_it_serves`) (167–266) → `svc_activity_ports/orchestrator_presence.rs` |
| 3 | `move_item`, `name: stores`, `outside` | `default_agent_tool_sockets`, `HostStores`, `build_host_stores` (`svc_host_builders.rs` 490–541) → `svc_host_builders/stores.rs` |
| 4 | `move_impl_members`, `name: svc_credential_builders` | `with_credential_vaults`, `with_github_cli`, `with_coder_credentials` (282–309) → `svc_host_builders/svc_credential_builders.rs`, beside `svc_room_builders.rs` |
| 5 | `move_item`, `name: roster_keepalive`, `named` | `ROSTER_KEEPALIVE_INTERVAL` with its doc comment (`connection_service.rs` 49–61) → `connection_service/roster_keepalive.rs` |

**Projected production lines**: `svc_activity_ports.rs` 546 → ~415; `hook_delivery.rs` ~40;
`orchestrator_presence.rs` ~115; `svc_host_builders.rs` 544 → ~467; `stores.rs` ~58;
`svc_credential_builders.rs` ~36; `connection_service.rs` 567 → 555 total (442 → 430 by
`restructure lines`); `roster_keepalive.rs` ~15.

**Row 2** takes two modules, not the one `hook_delivery.rs` State B names: the hook delivery is 32
lines and the file must lose 47, and its neighbours on both sides are the constants and the
`impl DaemonSessionHost` that builds the ports. The presence check is the cohesive 100 lines.

**Row 4.** The stack added nine lines to `connection_service.rs` (two struct fields with their doc
comments; the record's "+6" is the last PR's share), all inside `DaemonSessionHost`, which no
operation can shorten. The file is brought under its length at `3b15ffbc4` by moving the keepalive
constant: −13 lines and one blank, +1 `mod`, +1 `pub use`. The margin is three lines.
`DaemonRpcHandler` (42 lines) was the better seam and is not taken: `svc_host_builders.rs:155`
builds it by a struct literal over its private fields, which a move into `daemon_rpc_handler`
would put out of reach.

**Callers outside the crate**: none names a moved item by path (`ActivityHookDelivery` and
`ROSTER_KEEPALIVE_INTERVAL` are named only inside the crate; the builders are reached through
`DaemonSessionHost`). Inside the crate, operation 1 re-points `handler_state.rs:247`; operation 5
keeps `connection_service::ROSTER_KEEPALIVE_INTERVAL` through its `pub use`.

**Visibility**: `ServedTerminalPresence` and its fields `terminals`, `connection` private →
`pub(super)`; `default_agent_tool_sockets`, `HostStores` and its six fields, `build_host_stores`
private → `pub(super)`. Operations 1, 4 and 5 widen nothing.

**Refusals, verbatim, and what was tried next**

- Row 4 — operation 5 with `reexport: outside`:

  > this seam cannot be cut here: `use crate::{
  >     connection_service::ROSTER_KEEPALIVE_INTERVAL, multi_host::EligibleDaemonSource,
  >     CliSessionManager,
  > };` in packages/tddy-session-lifecycle/src/connection_service/svc_host_builders.rs names a moved item inside a nested group, so it cannot be re-pointed: write one `use` per path and plan again

  Next: `reexport: named`, which re-points nothing and passes.
- Row 16 — `svc_host_builders.rs`, three field expressions of the `Self { … }` literal: 106–112
  (`session_agent_rosters`), 87–89 (`agent_conversation_frames`), 65–67 (`sandbox_manager`). Each:

  > this seam cannot be cut here: rust-analyzer offers no "extract into function" assist for the given range (it offered: Extract into variable)

  Next: the statement range 45–54, which passes with `returns a 7-tuple; clippy::type_complexity
  may fire` and shortens nothing. Row 16 is not met.

## Plans D and D2 — `tddy-accounts` `service.rs`

`plan-D.jsonl`:

| # | Operation | What moves where |
|---|---|---|
| 1 | `move_item`, `name: link_answers`, `outside` | `store_link`, `state_only`, `pending`, `saturating_i64`, `link_status_for` (578–631) → `service/link_answers.rs` |
| 2 | `move_item`, `name: account_answers`, `outside` | `grouped_by_provider`, `requested_account`, `non_blank`, `refusal_of_provider_account`, `summary_of`, `sync_status_proto_of`, `status_for` (633–776) → `service/account_answers.rs` |
| 3 | `move_item`, `name: provider_routes`, `outside` | `route_option` (470–494) → `service/provider_routes.rs` |
| 4 | `move_impl_members`, `name: wiring` | the six builders `with_sync_status` … `with_clock` (90–159) → `service/wiring.rs` |

`plan-D2.jsonl`:

| # | Operation | What moves where |
|---|---|---|
| 1 | `move_impl_members`, `name: link_progress` | `session_account_of`, `answer_for`, `linked` (the whole third `impl`, 496–576) → `service/link_progress.rs` |

**Projected production lines** after both: `service.rs` 790 → ~425 (−55, −145, −26, −71, −82 moved;
five `mod` lines; about ten `use` lines for the names the trait impl writes bare);
`link_answers.rs` ~62; `account_answers.rs` ~158; `provider_routes.rs` ~30; `wiring.rs` ~82;
`link_progress.rs` ~93. After plan D alone `service.rs` is about 505, so D2 is what closes the row.

**The modules are children of `service`, not siblings of `linking.rs` and `provider_account.rs`** as
the backlog entry words it. The moved code reads the private fields of `AccountsServiceImpl`,
`Linking` and `ProviderAccountPorts`; a child of `service` reaches them with nothing widened, a
sibling could not. They are five modules and not the entry's two for rule 1 above: the helpers
are four separate runs in the file with the trait impl's users between them, and a module the
plan creates can take one run.

<a id="why-d-is-two-plans"></a>
**Why D is two plans.** `linked` and `answer_for` name `store_link`, `state_only`, `pending` and
`link_status_for`, so by rule 2 the helpers must move first. The `impl` sits immediately above
them, so by rule 3 it cannot move after them in the same plan. Drafted as one plan:

> Error: the index daemon refused this run (FailedPrecondition): anchor invalidated: position in packages/tddy-accounts/src/service.rs fell inside text removed by an earlier operation

Each of the two files passes on its own against the tree as it is. `plan-D2.jsonl` is applied after
`plan-D.jsonl`; its anchor names three members whose text plan D does not edit (plan D re-points
their bare names by adding `use` lines at the top of the file), so it stays valid.

**Callers outside the crate**: none. Every moved free function is private; `lib.rs` re-exports
`build_accounts_entry`, `AccountsServiceImpl` and `Clock`, which stay, and the builders are reached
through the type.

**Visibility**: `store_link`, `state_only`, `pending`, `saturating_i64`, `link_status_for`,
`grouped_by_provider`, `requested_account`, `refusal_of_provider_account`, `summary_of`,
`status_for`, `route_option` private → `pub(super)`; in D2 `AccountsServiceImpl::session_account_of`
and `::answer_for` private → `pub(super)`. The changeset's target of zero widenings for this group
cannot hold: the helpers leave the file whose trait impl calls them. `store_link` and
`saturating_i64` are widened only because their one caller moves a plan later; once D2 has run
they are called from a sibling module, which `pub(super)` is still the narrowest spelling for.

**Other drafts refused**

- Filling one module from two runs (`to` the module an earlier line created), four findings of
  the form:

  > `tddy_accounts::service::account_summaries` does not exist: `tddy_accounts::service` declares no module `account_summaries`. A move creates a module only when its line carries a `name`: `to` is then the parent the new module is declared in

  Next: one run per module.
- Moving `Linking` and `ProviderAccountPorts` to `service/ports.rs` first: the later run was
  refused `anchor invalidated: …` (its text reads `Linking`'s fields, whose declarations had
  moved). Next: the structs stay in `service.rs`, and the builders move instead.

## Plan E — `remote_git_service.rs`

| # | Operation | What moves where |
|---|---|---|
| 1 | `move_item`, `name: request`, `outside` | seam A: `GitVerb` and its `impl`, `open_from_first_frame`, `resolve_git_verb`, `resolve_project_repo`, `PackExecution`, `pack_execution_for_session`, `git_argv`, `git_child_command`, `GitChildCommand`, `git_argv_as_user`, `git_child_env`, `remote_pack_shell_command`, `git_child_argv`, `daemon_identity` (114–315) → `remote_git_service/request.rs` |
| 2 | `move_item`, `name: pumps`, `outside` | seam B, lower half: `OutputStream`, `PumpProgress`, `Pumps` with their impls, `pump_output`, `send_frame`, `supervise_child`, `supervise_streamed`, `drain_pumps`, `terminate_child`, `signal_process_group`, `exit_code_of` (849–1111) → `remote_git_service/pumps.rs` |
| 3 | `move_item`, `name: child_relay`, `outside` | seam B, upper half: `GitServerFrames` and `impl GitChildRelay` (618–847) → `remote_git_service/child_relay.rs` |

**Projected production lines**: `remote_git_service.rs` 1,111 → ~425 (−203, −264, −231 moved; three
`mod` lines; the facade; a few `use` lines); `request.rs` ~215; `pumps.rs` ~277;
`child_relay.rs` ~246.

**Seam B is two files, and the struct stays.** As the record designs it, seam B is lines 601–1111:
511 lines, which with its imports is a new file over budget. Cut in two, the halves name each
other (the relay spawns the pumps; the pumps take the relay's `FrameSender`), which rule 2
refuses in either order. So `struct GitChildRelay` and the `FrameSender` alias stay in
`remote_git_service.rs` (16 lines), the pumps move first, and the `impl` moves last into a child
module, where it still reads the struct's private fields with nothing widened.

**Callers outside the crate, absorbed by the `outside` facade** (a `pub use` in
`remote_git_service.rs` naming what something outside reaches; none of these files is edited):

| Item | Named from |
|---|---|
| `GitVerb`, `open_from_first_frame`, `resolve_git_verb`, `resolve_project_repo`, `git_argv`, `git_child_command`, `git_argv_as_user` | `tddy-worktree-service/tests/remote_git_{service,relay,livekit,ssh_clone,ssh_serve}_acceptance.rs` |
| `PackExecution`, `pack_execution_for_session` | `tddy-session-activity/src/remote_git_pack_execution.rs`, and the two ssh suites above |
| `GitServerFrames` | `tddy-worktree-service/tests/remote_pack_over_shell_acceptance.rs`, `remote_git_ssh_serve_acceptance.rs` |

`GitChildRelay`, `RemoteGitServiceImpl`, the resolver aliases, `PackExecutionContext` and the
constants stay where they are, so `tddy-daemon`, `tddy-daemon-livekit` and
`tddy_session_lifecycle::remote_git_service` (a re-export of the module) are untouched.
`GitChildCommand` has no reader outside the crate and loses its old public path.

**Visibility**: `remote_pack_shell_command`, `OutputStream`, `PumpProgress`, `Pumps` and its fields
`pumps`, `progress`, `pump_output`, `supervise_child`, `supervise_streamed` private →
`pub(super)`. Zero widenings, the changeset's target, cannot hold for a relay and its pumps in
two files.

**Refusals, verbatim**

- Seam B as `child_relay` filled by two runs and `pumps` by two:

  > `tddy_worktree_service::remote_git_service::pumps` does not exist: `tddy_worktree_service::remote_git_service` declares no module `pumps`. A move creates a module only when its line carries a `name`: `to` is then the parent the new module is declared in

  and the same for `child_relay`. Next: one run per module, the struct left in the parent.

## Plan F — `build_session_host`

| # | Operation | Range now | What it becomes |
|---|---|---|---|
| 1 | `extract_method` `session_reach_of` | 292 (the cast expression inside `with_session_reach(…)`) | the reach port as a trait object |
| 2 | `extract_method` `with_the_daemons_coder_credentials` | 276–284 | the Coder credential wiring the stack added |

**Projected**: `build_session_host` 82 → 72 (−8 and −2). `mount_host.rs` 482 → ~495 production
lines: both new functions stay in the file. **The margin under 500 is about five lines and is an
estimate.**

**Callers outside the crate**: none.

**The record's earlier refusals did not recur**: both ranges are offered "extract into function"
on this engine build, and neither signature was refused.

**One consented fix is expected after the apply.** Operation 2's range is a single
`let connection_impl = match … ;`, which rust-analyzer writes as `let connection_impl = …;
connection_impl`. `clippy::let_and_return` rejects that under `-D warnings`. It is the engine gap
recorded in `docs/dev/todo/2026-10-21-restructure-extract-leaves-a-let-and-return.md`, whose
remedy was a two-line lint fix the developer approved. Anchoring the expression instead, which
avoids it, was refused:

> rust-analyzer's answer was unusable: rust-analyzer left `connection_impl` behind in 2 place(s) its rename could not reach, so the extraction would report success over source that resolves nowhere — line(s) 268, 276, 278, 310, 312, 318, 389, 398, 405, 409, 414, 415, 419. That call sits inside an `impl`, and the module was written outside it, so no ordering of this plan makes the path resolve — an `impl` body cannot hold a `mod`. Either grow the seam to carry the whole `impl`, or cut it where nothing crosses.

**Tried and left out**: moving `RegisteredSubscriber` and its impl to a child module, to give the
file a wider margin. It passes `check --deep`, but `tddy-daemon/tests/unbundle_endpoint.rs` holds a
closed list of the files allowed under `tddy-daemon/src`, which a new
`runtime/mount_host/registered_subscriber.rs` is not on; and the check widened the struct without
its tuple field, which the constructor call in `mount_host.rs` needs. The wider ranges 265–284 and
263–284 also pass, and project the file at 497 to 499.

## Order of the applies

A and B are independent of each other and of the rest. C, D, E and F each touch one package.
D2 follows D. Nothing else is ordered. Each plan file is consumed by its apply.

## Group G — fixture warm-up in the two shared stub writers (hand edit, test code only)

The pattern is `write_executable` in
`packages/tddy-agent-launch/tests/remote_agent_launch_acceptance.rs:70`: the script carries
`if [ -n "$TDDY_FIXTURE_WARMUP" ]; then exit 0; fi` as its first line after `#!/bin/sh`, and the
writer runs it once with `TDDY_FIXTURE_WARMUP=1` and stdin closed, asserting it exits zero, before
it returns.

| Writer | Where the stub is written today | The edit |
|---|---|---|
| `a_stub_agent_script_minting` (`packages/tddy-session-lifecycle/tests/agent_registry_support/mod.rs:296`), installed as `acme` | `std::fs::write(&stub, …)` and `set_permissions` at `:557`–`:562` | the guard line goes between `#!/bin/sh` (`:313`) and the `create-chat` line (`:314`); after `set_permissions` the stub is run once, guarded |
| `a_stub_harness_script` (`packages/tddy-session-lifecycle/tests/harness_clis_support/mod.rs:116`), installed as the harness binary | `std::fs::write(&stub, …)` and `set_permissions` at `:323`–`:328` | the guard line goes between `#!/bin/sh` (`:132`) and the `touch "{marker}"` line (`:133`); after `set_permissions` the stub is run once, guarded |

The guard must come before the `create-chat` branch and before the `touch` of the spawn marker in
both scripts: a warm-up that touched the marker would report a spawn that never happened, and one
that reached `create-chat` would print a chat id. The warm-up run sets only `TDDY_FIXTURE_WARMUP`,
records no call and writes no pid file. Pinned by the suites that use these writers staying green
(`./test -p tddy-session-lifecycle`).

## Group H — `useDerivedHostSelection.ts` (hand split, TypeScript)

New file `packages/tddy-session-create-web/src/useDerivedHostSelection.ts`, exporting one hook:

```
useDerivedHostSelection(
  daemons: ReadonlyArray<{ instanceId: string; parent?: string }>,
  initialDaemonInstanceId: string,
): { daemonInstanceId: string; setDaemonInstanceId: (id: string) => void; sessionDaemonInstanceId: string }
```

It takes lines 118–128 of `useCreateSessionPaneState.tsx` as they are: the four-line comment and
the `useState` for `daemonInstanceId` (118–124), and the two-line comment, `selectedHostEntry` and
`sessionDaemonInstanceId` (125–128). The initial value's expression
(`projectHome?.daemonInstanceId || initialValues?.daemonInstanceId || selectedInstanceId || ""`)
stays in the caller and is passed in. The caller keeps one destructuring call and gains one
import: 11 lines out, 6 in, 503 → 498. `selectedHostEntry` is read nowhere else in the file. The
names the hook returns are the ones the file already uses, so no other line changes, and the
value `useCreateSessionPaneState` returns is unchanged.

`useCreateSessionPlacement.ts:77`–`:81` resolves the same derived host a second time to get
`sessionTarget`. Folding that into the new file is a larger change than the row needs and is not
part of this spec.

Pinned by `bun test src` (104 tests) and `cypress/component/CreateSessionDerivedHostAcceptance.cy.tsx`.

## Doc statements that name what moves (for the Final Checklist)

| Doc | What it says | After the plans |
|---|---|---|
| `packages/tddy-agent-launch/docs/module-layout.md:10` | `remote_agent_launch.rs` 546 with its record | the module and its two children |
| `packages/tddy-agent-launch/docs/module-layout.md:11`, `:47` | `svc_start_session_core.rs` 499; `record_the_started_sessions_name` | the method is in `svc_start_session_core/started_session_name.rs` |
| `packages/tddy-session-lifecycle/docs/session-service.md:56` | `record_the_started_sessions_name` in `start_session_on_request` | still true: called there, defined in the child module |
| `packages/tddy-session-lifecycle/docs/module-layout.md:38` | `connection_service.rs` holds `ROSTER_KEEPALIVE_INTERVAL` | it holds the re-export; the constant is in `roster_keepalive.rs` |
| `packages/tddy-session-lifecycle/docs/module-layout.md:39` | `svc_host_builders.rs` (children `rpc_activity.rs`, `svc_room_builders.rs`) | two more children: `stores.rs`, `svc_credential_builders.rs` |
| `packages/tddy-session-lifecycle/docs/module-layout.md:62` | `svc_activity_ports.rs` | two children: `hook_delivery.rs`, `orchestrator_presence.rs` |
| `packages/tddy-session-activity/docs/activity-service.md:109`, `:112` | `ServedTerminalPresence` | still true by name; its file is the child module |
| `packages/tddy-accounts/docs/provider-accounts.md:77` | `summary_of` | still true by name |
| `packages/tddy-daemon-kernel/docs/daemon-kernel.md:299` | `ROSTER_KEEPALIVE_INTERVAL * 2` | still true |
