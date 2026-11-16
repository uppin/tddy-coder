# Initial Discovery: the #remote-shell sweep — code issues and backlog the stack left

**Changeset**: [2026-11-16-remote-shell-sweep.md](./2026-11-16-remote-shell-sweep.md)
**Stack**: `#remote-shell 8/11` — `feature/remote-shell/sweep`
**Date**: 2026-11-16
**Passes**: 1

## Combined Conclusions

The seven PRs below this one (#915–#921) are implemented, wrapped and green. Between the stack's
merge-base (`3b15ffbc4`) and its top (`01a11360e`) they changed
301 files changed, 20862 insertions(+), 1114 deletions(-). Their wraps left code-issue records they were not
allowed to close, because a later PR of the stack also modified the file. This pass lists every
record the stack **added** or **made worse**, with its base and current measurement; the changeset
turns that list into seams.

- Four records were added: `remote_agent_launch.rs` (579), `svc_activity_ports.rs` (513 → 546),
  `useCreateSessionPaneState.tsx` (495 → 503), generated `host_pb.ts` (1,402 → 1,734).
- Twelve existing records were made worse; four more were touched and re-measured unchanged
  (`wire_session_agent`, `resumed_session_wiring`, `slots::render`, `unlock_and_add`) and are not
  in scope.
- Backlog entries: two added (`shared-stub-writers-fixture-warmup`,
  `coder-hosts-never-run-against-a-real-deployment`), one edited
  (`tddy-accounts-service-rs-over-file-budget`), one deleted as resolved.
- No `/analyze-code-issues` coverage run was made over the ~20 Rust packages the stack touched:
  every wrap ran its own code-issue gate and re-measured by brace matching, and those rows are the
  evidence used here. A CRAP-level finding the wraps' gates did not look for would not be in this list.

## Exploration 1: what the stack added and changed in the two records — 2026-11-16

**Agent**: parent git/grep
**Scope**: `docs/dev/todo/` and `packages/*/docs/code-issues/` between the stack's merge-base and its top

### Sequence

1. `git merge-base origin/master origin/feature/remote-shell/coder-hosts` — the stack's base.
2. `git diff --name-status <base>..<top> -- docs/dev/todo/` and `… | grep code-issues/` — the records touched.
3. `git show <top>:<record>` for each — header fields and the last measurement rows.

### Grep / glob

```
M	docs/dev/todo/2026-10-21-tddy-accounts-service-rs-over-file-budget.md
D	docs/dev/todo/2026-11-07-ssh-agent-launch-remote-pointer-owner.md
A	docs/dev/todo/2026-11-16-coder-hosts-never-run-against-a-real-deployment.md
A	docs/dev/todo/2026-11-16-shared-stub-writers-fixture-warmup.md
M	packages/tddy-accounts/docs/code-issues/oversized-file-service.md
M	packages/tddy-agent-launch/docs/code-issues/complexity-cli-agent-steps-resumed-session-wiring.md
M	packages/tddy-agent-launch/docs/code-issues/complexity-cli-agent-steps-wire-session-agent.md
M	packages/tddy-agent-launch/docs/code-issues/complexity-cli-branch-starts-start-cli-harness-from-request.md
M	packages/tddy-agent-launch/docs/code-issues/complexity-opencode-cli-spawn-resume-resume-cli-harness-session.md
M	packages/tddy-agent-launch/docs/code-issues/complexity-opencode-cli-spawn-resume-resume-cli-harness.md
M	packages/tddy-agent-launch/docs/code-issues/complexity-opencode-cli-spawn-start-cli-harness-session.md
M	packages/tddy-agent-launch/docs/code-issues/complexity-svc-start-sandboxed-cli-harness-session-start-sandboxed-cli-harness-session.md
M	packages/tddy-agent-launch/docs/code-issues/complexity-svc-start-session-core-start-session-core.md
A	packages/tddy-agent-launch/docs/code-issues/oversized-file-remote-agent-launch.md
M	packages/tddy-coding-agent-specs/docs/code-issues/complexity-slots-mod-render.md
M	packages/tddy-daemon/docs/code-issues/complexity-mount-host-build-session-host.md
M	packages/tddy-host-service/docs/code-issues/complexity-host-messages-unlock-and-add.md
A	packages/tddy-rust-typescript-tests/docs/code-issues/oversized-file-host-pb.md
A	packages/tddy-session-create-web/docs/code-issues/oversized-file-use-create-session-pane-state.md
M	packages/tddy-session-lifecycle/docs/code-issues/complexity-svc-host-builders-new.md
M	packages/tddy-session-lifecycle/docs/code-issues/oversized-file-connection-service.md
A	packages/tddy-session-lifecycle/docs/code-issues/oversized-file-svc-activity-ports.md
M	packages/tddy-session-lifecycle/docs/code-issues/oversized-file-svc-host-builders.md
M	packages/tddy-worktree-service/docs/code-issues/oversized-file-remote-git-service.md
```

### Inspected files

#### `packages/tddy-accounts/docs/code-issues/oversized-file-service.md`

```
**Location:** `packages/tddy-accounts/src/service.rs`
**Category:** oversized-file
**Metrics:** **790 production lines** · budget 500 · 1.6× over
**Thresholds breached:** length 790 > 500
**Restructure:** not designed
**Status:** Open — unclaimed. Already over budget before #754; #916 grew it 772 → 790 and deferred the split to the follow-up named in the measurement history
| 2026-11-16 | 790 | `#remote-shell` 2/7 (#916): 772 → 790 — one delegating `connect_coder_account` arm, the `coder` field and two imports; every Coder helper lives in `coder_account.rs`. Not split here: that PR's boundaries forbid it and the file is inherited by every later `#remote-shell` node. The deferred split is `docs/dev/todo/2026-10-21-tddy-accounts-service-rs-over-file-budget.md` |
```

#### `packages/tddy-agent-launch/docs/code-issues/complexity-cli-agent-steps-resumed-session-wiring.md`

```
**Location:** `packages/tddy-agent-launch/src/cli_agent_steps.rs:465` — `LaunchSessions::resumed_session_wiring`
**Category:** complexity
**Metrics:** **82 lines** (fn line to closing brace)
**Thresholds breached:** length 72 > 60 (`/analyze-clean-code`)
**Status:** Open — regressed 2026-11-15 by #911 (80 → 82): the `resumed_argv_hooks` call and the `hooks` field; the logic lives in the new helper — unclaimed
| 2026-11-15 | 80 → 82 | **worse by 2** in #911: the `resumed_argv_hooks` call and the `hooks` field; the logic lives in the new helper beside `claude_cli_hooks`. Brace-matched on `origin/master` (`66bb45213`) and HEAD at #911's wrap |
| 2026-11-16 | 82 → 82 | #921 (`#remote-shell` 7/7): re-measured, brace-matched on `origin/feature/remote-shell/remote-hooks` (`529e4c653`) and HEAD — unchanged; this PR's `cli_agent_steps` growth is the shell slot writer beside it |
```

#### `packages/tddy-agent-launch/docs/code-issues/complexity-cli-agent-steps-wire-session-agent.md`

```
**Location:** `packages/tddy-agent-launch/src/cli_agent_steps.rs:359` — `wire_session_agent`
**Category:** complexity
**Metrics:** **84 lines** (fn line to closing brace)
**Thresholds breached:** length 80 > 60 (`/analyze-clean-code`)
**Status:** Open — partially fixed 2026-11-07 by #834 (87 → 84), still over 60 — unclaimed
| 2026-11-16 | 84 → 84 | #921 (`#remote-shell` 7/7): re-measured, brace-matched on `origin/feature/remote-shell/remote-hooks` (`529e4c653`) and HEAD — unchanged; this PR's `cli_agent_steps` growth is the shell slot writer beside it |
```

#### `packages/tddy-agent-launch/docs/code-issues/complexity-cli-branch-starts-start-cli-harness-from-request.md`

```
**Location:** `packages/tddy-agent-launch/src/cli_branch_starts.rs:144` — `start_cli_harness_from_request`
**Category:** complexity
**Metrics:** **92 lines**
**Thresholds breached:** length 61 > 60 (`/analyze-clean-code`)
**Restructure:** not designed — deferred to the #agent-tools sweep node (developer, 2026-11-02)
**Status:** Open — regressed 2026-11-07 by #834 (89 → 92) — unclaimed
| 2026-11-16 | 98 → 100 | #921 (`#remote-shell` 7/7): the two branch starts hand the start its caller subject and Coder credentials. Touched, not closed |
```

#### `packages/tddy-agent-launch/docs/code-issues/complexity-opencode-cli-spawn-resume-resume-cli-harness-session.md`

```
**Location:** `packages/tddy-agent-launch/src/opencode_cli_spawn/resume.rs` — `resume_cli_harness_session`
**Category:** complexity
**Metrics:** **136 lines**
**Thresholds breached:** length 74 > 60 (`/analyze-clean-code`)
**Restructure:** not designed — deferred to the #agent-tools sweep node (developer, 2026-11-02)
**Status:** Open — regressed 2026-11-15 by #911 (131 → 136): the toolset take, the `hook_params` call, the hooks slot field and its comment, mirroring the start path — unclaimed
| 2026-11-15 | 131 → 136 | **worse by 5** in #911: argv hooks render again on resume (the toolset take, the `hook_params` call, the hooks slot field, a two-line comment), mirroring the start path; the 131 base is growth since #891. Brace-matched on `origin/master` (`66bb45213`) and HEAD at #911's wrap |
| 2026-11-16 | 157 → 166 | #921 (`#remote-shell` 7/7): a resume resolves its target's shell as its caller, so a locked vault refuses it before anything runs. Touched, not closed |
```

#### `packages/tddy-agent-launch/docs/code-issues/complexity-opencode-cli-spawn-resume-resume-cli-harness.md`

```
**Location:** `packages/tddy-agent-launch/src/opencode_cli_spawn/resume.rs` — `LaunchSessions::resume_cli_harness`
**Category:** complexity
**Metrics:** **88 lines**
**Thresholds breached:** length 88 > 60 (`/analyze-clean-code`)
**Restructure:** not designed — deferred to the `#agent-tools` sweep node (n20) by the developer (2026-11-04)
**Status:** Open — unclaimed
| 2026-11-16 | 95 → 108 | #921 (`#remote-shell` 7/7): the resume path carries the session's subject and Coder credentials to the target resolution. Touched, not closed |
```

#### `packages/tddy-agent-launch/docs/code-issues/complexity-opencode-cli-spawn-start-cli-harness-session.md`

```
**Location:** `packages/tddy-agent-launch/src/opencode_cli_spawn.rs:133` — `start_cli_harness_session`
**Category:** complexity
**Metrics:** **302 lines**
**Thresholds breached:** length 191 > 60 (`/analyze-clean-code`)
**Restructure:** not designed — deferred to the #agent-tools sweep node (developer, 2026-11-02)
**Status:** Open — regressed 2026-11-07 by #834 (300 → 302) — unclaimed
| 2026-11-16 | 319 → 333 | **worse by 14** in #917 (`#remote-shell` 3/7), brace-matched on `origin/feature/remote-shell/coder-account` (`87e84fb27`) and HEAD; no coverage run. The target wiring the changeset allows: the `target` field, the checkout-alias selection and the one `HarnessRemote` call. Not extracted: restructuring stays with the `#agent-tools` sweep; this file is also touched by successor stacks |
| 2026-11-16 | 347 → 368 | **worse by 21** in #921 (`#remote-shell` 7/7), brace-matched on `origin/feature/remote-shell/remote-hooks` (`529e4c653`) and HEAD after its wrap split; no coverage run. The Coder target resolution before the checkout is cut, and the slot files and pointer through the resolved shell. At the same wrap `CliSessionStart` and `shell_for_session_target` moved to `opencode_cli_spawn::session_start` (file 529 → 422 production lines); this function stays with the `#agent-tools` sweep as deferred |
```

#### `packages/tddy-agent-launch/docs/code-issues/complexity-svc-start-sandboxed-cli-harness-session-start-sandboxed-cli-harness-session.md`

```
**Location:** `packages/tddy-agent-launch/src/svc_start_sandboxed_cli_harness_session.rs:92` — `LaunchSessions::start_sandboxed_cli_harness_session`
**Category:** complexity
**Metrics:** **278 lines** (fn line to closing brace) · CRAP not measured (no `analyze coverage` run)
**Thresholds breached:** length 214 > 60 (`/analyze-clean-code`)
**Restructure:** required — `extract_method` × 4, `/code-restructuring` territory, blocked as below
**Status:** Open — regressed 2026-11-06 by #823 (239 → 278) — unclaimed
| 2026-11-16 | 291 → 292 | #921 (`#remote-shell` 7/7): the sandboxed start hands on the Coder credential source. Touched, not closed |
```

#### `packages/tddy-agent-launch/docs/code-issues/complexity-svc-start-session-core-start-session-core.md`

```
**Location:** `packages/tddy-agent-launch/src/svc_start_session_core.rs:64` — `start_session_on_request` (was `start_session_core`, renamed by #669), and `start_here` (`:145`), which holds its per-session-type body since #751
**Category:** complexity
**Metrics:** **842 lines** · **nesting depth 6** · 1 parameters · 40 branch/match lines · 54 early exits
**Thresholds breached:** length 842 > 60; nesting 6 > 4 (`/analyze-clean-code`)
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — partially fixed 2026-11-06 by #823 (`start_here` 348 → 265); both functions still over 60 — unclaimed
| 2026-11-15 | `start_session_on_request` 73 → 79, `start_here_body` 274 → 274 | — | — | — | **regressed** by session display names (#910), fn line to closing brace on `9712eb8d9~1` and `9712eb8d9`: `refuse_a_named_tool_start` beside the orchestrated-tool refusal, and `start_here`'s result recorded before it is returned so the requested name can be written. The write itself is its own method, `record_the_started_sessions_name` (33 lines); `start_here` (32) and `start_here_body` unchanged. Nesting, branches and exits not re-derived; no coverage run. Kept open |
| 2026-11-16 | `start_session_on_request` 79 → 80, `start_here_body` 274 → 274 | — | — | — | **worse by 1** in #917 (`#remote-shell` 3/7): the one `refuse_a_target_combination(&req)?` call the changeset allows; brace-matched on `origin/feature/remote-shell/coder-account` (`87e84fb27`) and HEAD; no coverage run. Kept open |
```

#### `packages/tddy-agent-launch/docs/code-issues/oversized-file-remote-agent-launch.md`

```
**Location:** `packages/tddy-agent-launch/src/remote_agent_launch.rs`
**Category:** oversized-file
**Metrics:** **451 → 546 production lines** (2026-11-16) · budget 500
**Thresholds breached:** length 546 > 500
**Restructure:** `extract_module` / `move_item` into a submodule — `/code-restructuring` territory
**Status:** Open — unclaimed
| 2026-11-16 | 451 → 546 | #920 (`#remote-shell` 6/7): the pid-based startup window (`wait_for_agent_start`, `AgentStart`, `AgentNeverStarted`) and the hook-log creation in `launch_on_target`. **Not split**: successor PR #921 (`feature/remote-shell/coder-hosts`) also modifies this file, and a split here would turn its diff into a conflict (`/pr-wrap` step 3.5, stack-branch rule). Split after the `#remote-shell` stack lands |
| 2026-11-16 | 546 → 579 | #921 (`#remote-shell` 7/7): the Coder case of `session_target_of_proto` / `proto_of_session_target` and the `Coder` arm's refusal in `shell_for_target`. Re-measured at #921's wrap and left for the sweep after the stack lands, as recorded at #920 |
```

#### `packages/tddy-coding-agent-specs/docs/code-issues/complexity-slots-mod-render.md`

```
**Location:** `packages/tddy-coding-agent-specs/src/slots/mod.rs:101` — `render`
**Category:** complexity
**Metrics:** **66 lines**
**Thresholds breached:** length 65 > 60 (`/analyze-clean-code`)
**Restructure:** not designed — deferred to the #agent-tools sweep node (developer, 2026-11-02)
**Status:** Open — partially fixed (the hooks arm is extracted; the MCP arm extraction still closes it) — unclaimed
| 2026-11-15 | 69 → 66 | **better by 3** in #911: the hooks arm moved into `render_hooks_slot` beside `render_tool_slot`; brace-matched on `origin/master` (`66bb45213`) and HEAD at #911's wrap |
| 2026-11-16 | 66 (unchanged) | touched by #920 (`#remote-shell` 6/7): three test literals in the module gained `remote_log: None`; `render` itself gained no branch — the carrier is a field of the params it passes through. Brace-matched at #920's wrap |
```

#### `packages/tddy-daemon/docs/code-issues/complexity-mount-host-build-session-host.md`

```
**Location:** `packages/tddy-daemon/src/runtime/mount_host.rs:227` — `Runtime::build_session_host`
**Category:** complexity
**Metrics:** **74 lines** (2026-11-10) · nesting depth 1 · 1 branch keyword · 5 parameters (+ `&self`) · 0 early returns
**Thresholds breached:** length 74 > 60 (`/analyze-clean-code`); depth and complexity within budget
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — deferred by the user, 2026-11-10 (#876 sweep); unclaimed
| 2026-11-16 | 73 → 73 | 1 | #918 (`#remote-shell` 4/7): re-measured — `build_session_host` unchanged. The file grew 499 → 521 with the Coder wiring, then the sweep pair moved to `runtime/last_run_sweep.rs` and the Coder wiring to `coder_credential_source::wire_coder` in the same PR, leaving it at 473 production lines. This finding is untouched and stays open |
| 2026-11-16 | 73 → 82 | #921 (`#remote-shell` 7/7): `build_session_host` wires the daemon's Coder credential source into the connection host when vaults are present. Touched, not closed — left as deferred at #876 |
```

#### `packages/tddy-host-service/docs/code-issues/complexity-host-messages-unlock-and-add.md`

```
**Location:** `packages/tddy-host-service/src/host_messages.rs:118` — `unlock_and_add`
**Category:** complexity
**Metrics:** **80 lines** · **nesting depth 4** · 1 parameters · 5 branch/match lines · 3 early exits
**Thresholds breached:** length 80 > 60 (`/analyze-clean-code`)
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — **unclaimed**
| 2026-11-16 | 80 | 4 | 5 | 3 | #918 (`#remote-shell` 4/7): re-measured — unchanged. That PR added `coder_cli_message` elsewhere in the same file and did not touch `unlock_and_add`; the finding stays open exactly as it stood |
```

#### `packages/tddy-rust-typescript-tests/docs/code-issues/oversized-file-host-pb.md`

```
**Location:** `packages/tddy-rust-typescript-tests/gen/host_pb.ts`
**Category:** oversized-file
**Metrics:** 1,734 production lines (whole file; TypeScript has no `#[cfg(test)]` cut) — 1,402 at the base of #918, grown by that PR's and #919's regenerations
**Thresholds breached:** 1,734 > 500 production-line budget
**Restructure:** none — generated output
**Status:** Open — alert only; do not split
| 2026-11-16 | 1,402 → 1,571 | #918 (`#remote-shell` 4/7): `host.proto` gained `ListCoderWorkspaces` and `HostCoderCli`; both `gen/host_pb.ts` copies regenerated |
| 2026-11-16 | 1,571 → 1,734 | #919 (`#remote-shell` 5/7): `host.proto` gained `ListDerivedHosts`, `DerivedHost` and `DerivedHostKind`; both `gen/host_pb.ts` copies regenerated. Alert only — generated, and successors #920/#921 also modify this file |
```

#### `packages/tddy-session-create-web/docs/code-issues/oversized-file-use-create-session-pane-state.md`

```
**Location:** `packages/tddy-session-create-web/src/useCreateSessionPaneState.tsx`
**Category:** oversized-file
**Metrics:** **503 production lines** (whole file; TypeScript has no `#[cfg(test)]` cut) — 495 at the base of #919, grown 8 by that PR
**Thresholds breached:** 503 > 500 production-line budget
**Restructure:** hand split (TypeScript — the `code-restructuring` engine is Rust-only)
**Status:** Open — unclaimed
| 2026-11-16 | 495 → 503 | #919 (`#remote-shell` 5/7): the Host field holds a derived host's id, resolved to its parent daemon and target for the start request. **Not split**: successor PR #920 (`feature/remote-shell/remote-hooks`) also modifies this file, and a split here would turn its diff into a conflict (`/pr-wrap` step 3.5, stack-branch rule). Split after the `#remote-shell` stack lands |
```

#### `packages/tddy-session-lifecycle/docs/code-issues/complexity-svc-host-builders-new.md`

```
**Location:** `packages/tddy-session-lifecycle/src/connection_service/svc_host_builders.rs:33` — `DaemonSessionHost::new`
**Category:** complexity
**Metrics:** **97 lines** (2026-11-10) · nesting depth 1 · 0 branch keywords · 10 parameters · 0 early returns
**Thresholds breached:** length 97 > 60 (`/analyze-clean-code`); depth and complexity within budget
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — deferred by the user, 2026-11-10 (#876 sweep); unclaimed
| 2026-11-16 | 97 → 99 | 1 | #920 (`#remote-shell` 6/7): the host gains its remote-hook followers registry. Touched, not closed — the file is also modified by successor PR #921, so no extract is attempted here (`/pr-wrap` step 3.5, stack-branch rule) |
| 2026-11-16 | 99 → 100 | #921 (`#remote-shell` 7/7): the host's `coder_credentials` field defaults to `NoCoderAccounts` in `new`. Touched, not closed — left for the sweep after the stack lands |
```

#### `packages/tddy-session-lifecycle/docs/code-issues/oversized-file-connection-service.md`

```
**Location:** `packages/tddy-session-lifecycle/src/connection_service.rs`
**Category:** oversized-file
**Metrics:** **525 production lines** by the changeset's counter (2026-10-09; 582 on `origin/master` before #536; the 508 / 503 below are an earlier counter) · budget 500 (the inline-test-block rule; the naive count to the first `#[cfg(test)]` reads 18)
**Thresholds breached:** length 525 > 500
**Restructure:** group the facade blocks by receiver (see "What would close it"); #536 has landed the moves, so the re-measure the earlier note waited for is done: 525
**Status:** Open — regressed 2026-10-24 (+7 from #721, `#coagents` 2/8: the `coding_agent_launcher` field and a re-export; split deferred because `#coagents` 4/8 touches this file); narrowed 2026-10-21 (−29 from #635, `#satellite` 13/13); regressed 2026-10-20 (+5 from #634, `#satellite` 12/13: the `split_rooms` field; split deferred because #633 and #635 touch this file); regressed 2026-10-19 (+4 
| 2026-11-16 | +6 | #921 (`#remote-shell` 7/7), by `git diff --numstat` (6 added, 0 removed; the record's counter not re-run): the host's Coder credential field and its re-exports. Still over 500 by the record's counter; not split here |
```

#### `packages/tddy-session-lifecycle/docs/code-issues/oversized-file-svc-activity-ports.md`

```
**Location:** `packages/tddy-session-lifecycle/src/connection_service/svc_activity_ports.rs`
**Category:** oversized-file
**Metrics:** **513 → 546 production lines** (2026-11-16) · budget 500
**Thresholds breached:** length 546 > 500
**Restructure:** `move_item` / `move_impl_members` into a submodule — `/code-restructuring` territory
**Status:** Open — unclaimed
| 2026-11-16 | 513 → 546 | #920 (`#remote-shell` 6/7): `ActivityHookDelivery`, the in-process delivery of a target session's followed hooks. Already over the budget at the base; this PR grew it. **Not split**: successor PR #921 (`feature/remote-shell/coder-hosts`) also modifies this file, and a split here would turn its diff into a conflict (`/pr-wrap` step 3.5, stack-branch rule). Split after the `#remote-shell` stack lands |
```

#### `packages/tddy-session-lifecycle/docs/code-issues/oversized-file-svc-host-builders.md`

```
**Location:** `packages/tddy-session-lifecycle/src/connection_service/svc_host_builders.rs`
**Category:** oversized-file
**Metrics:** **493 → 531 production lines** (2026-11-10) · budget 500
**Thresholds breached:** length 531 > 500
**Restructure:** `move_item` / `move_impl_members` into a submodule — `/code-restructuring` territory
**Status:** Open — deferred by the user, 2026-11-10 (#876 wrap); unclaimed
| 2026-11-16 | 531 → 533 | #920 (`#remote-shell` 6/7): the host gains the remote-hook followers registry (2 lines). Touched, not closed. **Not split**: successor PR #921 (`feature/remote-shell/coder-hosts`) also modifies this file (`/pr-wrap` step 3.5, stack-branch rule). Split after the `#remote-shell` stack lands |
| 2026-11-16 | 533 → 544 | #921 (`#remote-shell` 7/7): the host gains its Coder credential source (`with_coder_credentials` and the `NoCoderAccounts` default). Re-measured at #921's wrap and left for the sweep after the stack lands, as recorded at #920 |
```

#### `packages/tddy-worktree-service/docs/code-issues/oversized-file-remote-git-service.md`

```
**Location:** `packages/tddy-worktree-service/src/remote_git_service.rs`
**Category:** oversized-file
**Metrics:** **1,111 production lines** (1,111 total — the file has no `#[cfg(test)]` module) · budget 500 · **2× over**
**Thresholds breached:** length 1111 > 500
**Restructure:** `extract_module --to_file` — seams designed below, not applied
**Status:** Open — **unclaimed** · regressed 2026-11-16 by #915 (1,004 → 1,111: the streamed remote-shell port path added more than the deleted `ssh_batch_mode_argv` spawn branch removed) — split deferred to a follow-up after the `#remote-shell` stack lands because successor #916 also touches this file
| 2026-11-16 | 1,111 | #915 (`#remote-shell` 1/7): `ssh_batch_mode_argv` deleted, remote pack verbs stream through `tddy-remote-shell` (`spawn_remote_pack_verb`, `supervise_streamed`) — net +107. Not split: the PR's boundaries forbid it and #916 touches the file, so a split here would cascade conflicts through the stack. Follow-up after the stack lands |
```

### Findings

| # | Record | Base → now | In scope |
|---|---|---|---|
| 1 | `tddy-agent-launch` `oversized-file-remote-agent-launch` | new → 579 | yes — added |
| 2 | `tddy-session-lifecycle` `oversized-file-svc-activity-ports` | 513 → 546 | yes — added record, file worse |
| 3 | `tddy-session-lifecycle` `oversized-file-svc-host-builders` | 531 → 544 | yes — worse |
| 4 | `tddy-session-lifecycle` `oversized-file-connection-service` | +6 lines | yes — worse |
| 5 | `tddy-accounts` `oversized-file-service` | 772 → 790 | yes — worse |
| 6 | `tddy-worktree-service` `oversized-file-remote-git-service` | 1,004 → 1,111 | yes — worse |
| 7 | `tddy-session-create-web` `oversized-file-use-create-session-pane-state` | 495 → 503 | yes — added (TypeScript, hand split) |
| 8 | `tddy-rust-typescript-tests` `oversized-file-host-pb` | 1,402 → 1,734 | generated output — no seam exists |
| 9 | `start_cli_harness_session` | 319 → 368 | yes — worse by 49 |
| 10 | `resume_cli_harness_session` | 157 → 166 | yes — worse by 9 |
| 11 | `resume_cli_harness` | 95 → 108 | yes — worse by 13 |
| 12 | `build_session_host` (`tddy-daemon`) | 73 → 82 | yes — worse by 9 |
| 13 | `start_cli_harness_from_request` | 98 → 100 | worse by 2 |
| 14 | `start_sandboxed_cli_harness_session` | 291 → 292 | worse by 1 |
| 15 | `start_session_on_request` | 79 → 80 | worse by 1 |
| 16 | `DaemonSessionHost::new` | 97 → 100 | worse by 3 |

The `#agent-tools` sweep (#876) scoped its function tier to functions grown by five or more lines;
rows 13–16 are below that. Rows 12 and 16 carry recorded engine refusals from that sweep for their
remaining seams (a builder chain and a struct literal that offer only "extract into variable").
