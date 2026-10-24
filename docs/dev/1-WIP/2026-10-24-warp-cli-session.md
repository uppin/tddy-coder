# Changeset: Warp CLI session (`warp-cli`) that resumes its conversation

**Date**: 2026-10-24
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — five packages (agent-backend, agent-launch, changeset, session-activity, web);
a spawned-process seam (PTY exit watcher reading the capture after `pty_done`); exit-vs-read ordering;
a `SessionMetadata` field touching ~50 struct literals
**Stack**: `#warp-agent` 1/2 — branch `feature/warp-agent/warp-cli-session`, base `master`
**PR**: [#717](https://github.com/uppin/tddy-coder/pull/717)

## Initial Discovery

[2026-10-24-warp-cli-session-initial-discovery.md](./2026-10-24-warp-cli-session-initial-discovery.md)
— Warp CLI facts, the developer's interview answers, the CLI-harness wiring map (Exploration 2) and
the terminal-tab map (Exploration 3, for the successor node).

## Prerequisites

The code issues and TODOs this node runs into. Step 2b found **no** `**Claimed by:**` record in any
package this node touches.

### ⚠ DURING — `packages/tddy-agent-backend/docs/code-issues/oversized-file-backend-mod.md`

`backend/mod.rs` is at 906 production lines against a 500-line budget, unclaimed. This node adds the
one-line `"warp"` row of `context_globs_for_agent`. Re-measure at wrap and record the +1. Recorded,
not fixed here: the split is large and orthogonal to this feature.

### ⚠ DURING — `packages/tddy-agent-launch/docs/code-issues/complexity-svc-start-session-core-start-session-core.md`

`start_session_on_request` is over budget. Since #667, `warp-cli` reaches it through the existing
registry lookup (`cli_harness(req.session_type)`). This node **must not add a branch** there. The
provider-account refusal belongs in the generic CLI-harness start path or in `route_supported`.

### ⚠ DURING — `packages/tddy-agent-launch/docs/code-issues/complexity-svc-resume-session-resume-session-at-session-coordinate.md`

Same constraint on resume. The token goes in through `resume_cli_harness_session`, never through a
new session-type arm in `resume_session_at_session_coordinate`.

### ⚠ DURING — [2026-08-13-session-deletion-leaks-the-worktree-for-every-session-type-except-clau.md](../todo/2026-08-13-session-deletion-leaks-the-worktree-for-every-session-type-except-clau.md)

`worktree_removal_applies_to` now lists claude-cli, opencode-cli, muse-cli and workspace. This node
adds `warp-cli`, so the new type does not join the leakers. Cursor still leaks, and that entry stays
open.

### ℹ PRECEDENT — [2026-10-22-muse-code-cli-facts-unconfirmed.md](../todo/2026-10-22-muse-code-cli-facts-unconfirmed.md)

Warp's flags come from Warp's official reference, not secondary sources. There is still no real
`warp` binary on the development machine (Warp.app bundles only `oz`), so the same "pinned as
documented" risk applies. The exit-line format is the riskiest fact. A `TODO(warp-cli-facts)` and a
backlog entry go in at wrap.

Packages **not analyzed** (no `docs/code-issues/`): `tddy-changeset`.

## Responsibility

This PR owns the **`warp-cli` session type**, end to end:

- `packages/tddy-agent-backend/src/cli_harness_warp.rs`: `WARP_CLI` (the descriptor), `warp_argv`,
  and `resume_token_from_output(&[u8]) -> Option<String>` (the exit-line parser).
- `Harness::WarpCli` in `provider_route.rs`, which accepts **no** preset.
- The `"warp"` row of `context_globs_for_agent`.
- `warp_cli_models()`, an empty curated catalogue.
- `SessionMetadata.harness_resume_token: Option<String>`.
- The **resume-token watcher**. After a CLI-harness PTY launch, it waits for `pty_done`, reads the
  capture and records the token, at start and at resume.
- Token-carrying generic resume: `CliLaunch.conversation_id` comes from `harness_resume_token`.
- `worktree_removal_applies_to("warp-cli")`.
- Web: the `warp-cli` `SessionType`, its toggle entry and request branch, no model field, Create
  enabled without a model.

## Boundaries

- **No agent terminal tabs.** `StartTerminalSession`, `TerminalRoster`, the tab strip and
  `useSessionTerminals` belong to the successor node (`feature/warp-agent/agent-terminal-tabs`).
- **No provider preset** (`tddy-accounts` untouched), and no Accounts-screen or **Default for** row.
- **No sandbox**. `sandboxed_agent_kind` stays without a `warp-cli` arm, and sandbox-recipes,
  sandbox-runner and sandbox-app are untouched.
- **No model selection**, no `--auto-approve`, no first prompt.
- No new arm in `start_session_on_request` or `resume_session_at_session_coordinate` (see
  Prerequisites).
- The watcher records only a token that its own parser recognises. It never rewrites any other
  metadata field.

## Dependencies

This is the stack's root node. It has **no parent node** and consumes nothing unshipped: every
surface it builds on is on `master` (CLI harness registry #667; generic start/resume
`opencode_cli_spawn.rs`; `PtyHandle.{pty_done, capture}`).

## Draft PR contract

The second commit (wave 2) publishes the surface below. Bodies are `todo!()` / `TODO(warp-cli-session)`.
Failing tests come with it. The successor node compiles against the descriptor without calling it.

- `pub const WARP_CLI: CliHarness` registered in `CLI_HARNESSES` (`[CliHarness; 5]`). The descriptor
  values are real; `argv: warp_argv` has a `todo!()` body.
- `pub fn resume_token_from_output(output: &[u8]) -> Option<String>` (`todo!()`).
- `pub fn warp_cli_models() -> BackendModels` (`todo!()`).
- `Harness::WarpCli` with its `id()` / `ALL` / `product()` arms, and Cursor's `HarnessCannotBeRouted`
  arm in `route_supported` / `route_body`. These are real: a `todo!()` would panic every existing test
  that walks `Harness::ALL`.
- `SessionMetadata.harness_resume_token`, added with every literal site updated (`None`). This is real,
  not stubbed: it is a field, and the workspace must compile.
- Web: `SessionType` includes `"warp-cli"`. The toggle entry is real, and the request/model-optional
  behaviour is pinned by a failing Cypress spec.
- Every acceptance test below, failing for this node's missing implementation.

## Green wave

**Wave:** 1 of 1
**Greenable independently:** yes. Every test drives this node's own surface against a stub `warp`
script; nothing exercises a parent.
**Concurrent with:** the successor (`feature/warp-agent/agent-terminal-tabs`), whose tests use
existing harnesses (Muse, OpenCode), not Warp.
**Blocks:** nothing by behaviour. The successor sits above this node only because `gh stack` needs a
line. Once both land, Warp appears in the successor's tab menu because the registry lists it.

Real dependency edges: none (`n1`, `n2` independent; the line `n1 → n2` is flattening only).

## Affected Packages

- **tddy-agent-backend**: [README.md](../../../packages/tddy-agent-backend/README.md) · [docs/architecture.md](../../../packages/tddy-agent-backend/docs/architecture.md): the fifth descriptor, `Harness::WarpCli`, the resume-token parser
- **tddy-agent-launch**: [README.md](../../../packages/tddy-agent-launch/README.md) · [docs/module-layout.md](../../../packages/tddy-agent-launch/docs/module-layout.md): the resume-token watcher; token-carrying generic resume; the provider-account refusal
- **tddy-changeset**: [README.md](../../../packages/tddy-changeset/README.md): `SessionMetadata.harness_resume_token`
- **tddy-session-activity**: [README.md](../../../packages/tddy-session-activity/README.md): worktree removal includes `warp-cli`
- **tddy-session-lifecycle** (tests only): `tests/warp_cli_session.rs`
- **tddy-web**: [README.md](../../../packages/tddy-web/README.md) · [docs/create-session-provider.md](../../../packages/tddy-web/docs/create-session-provider.md): Warp in the session-type toggle, no model field
- Mechanical literal updates (`harness_resume_token: None`) in tddy-session-split, tddy-session-files, tddy-telegram-control, tddy-testing-commons, tddy-daemon-rpc, tddy-daemon and the crates' tests. No behaviour change there.

## Related Feature Documentation

- [PRD-2026-10-24-warp-cli-session.md](../../ft/daemon/1-WIP/PRD-2026-10-24-warp-cli-session.md)
- [Muse Code CLI session](../../ft/daemon/muse-cli-session.md): the precedent
- [Terminal sessions](../../ft/daemon/terminal-sessions.md), [Provider accounts](../../ft/daemon/provider-accounts.md)

## Summary

Adds a fifth CLI harness, `warp-cli`, which runs Warp's interactive Agent CLI in a daemon-cut
worktree. Warp prints its resume token only when it exits. A watcher on the harness PTY reads that
line from the capture tail after every exit and records it in `.session.yaml`. Resume passes it back
as `warp --resume <TOKEN>`.

## Background

See the PRD. Technically, the CLI harness registry (#667) makes a harness one descriptor plus a
handful of string lists. Warp breaks two of the assumptions earlier harnesses could rely on:

1. **No model.** The web always required one (`useCreateSessionSubmit.ts:60`). The routed path
   refuses an empty one too (`session_provider_route.rs:95`), but this node takes no provider
   account, so the routed path is never reached.
2. **The resume handle is an output of the run.** Cursor's chat id is minted *before* the spawn
   (`agent create-chat`). Warp's token exists only after `warp` exits. Nothing in the repo reads PTY
   output after exit today.

## Scope

- [ ] **Descriptor**: `WARP_CLI`, `warp_argv`, `warp_cli_models`, registered fifth
- [ ] **Route**: `Harness::WarpCli` refusing every preset as Cursor is refused (`HarnessCannotBeRouted`). A routed start checks routability **before** the empty-model check, so the refusal names Warp
- [ ] **Context**: `"warp"` globs `AGENTS.md`, `.agents/**`, `WARP.md`
- [ ] **Metadata**: `harness_resume_token` field plus literal sites
- [ ] **Token capture**: parser plus watcher at start and resume
- [ ] **Resume**: token-carrying generic resume; fresh start when none
- [ ] **Delete**: worktree removal includes `warp-cli`
- [ ] **Web**: toggle, request, no model field, Create enabled without a model
- [ ] **Docs**: `docs/ft/daemon/warp-cli-session.md` at wrap; backlog entries for jailed Warp, a Warp provider account, unconfirmed Warp facts, and `--auto-approve`

## Technical Changes

### State A (Current)

- `CLI_HARNESSES: [CliHarness; 4]`, holding claude-cli, cursor-cli, opencode-cli and muse-cli. The
  descriptor fields are `session_type, label, route_harness, context_agent, default_binary,
  binary_env, route_config_env, argv, models`.
- `CliLaunch.conversation_id` is used only by `cursor_cli_argv`. The generic resume
  (`opencode_cli_spawn::resume_cli_harness_session`, `opencode_cli_spawn.rs:337-341`) passes
  `CliLaunch { binary, resume: true, ..default }`.
- `opencode_cli_spawn::launch()` (363-384) returns the main `Arc<PtyHandle>`, then drops it. Both
  callers have `session_dir` in scope. `PtyHandle.pty_done` flips to `true` when the task reaches a
  terminal status. `PtyHandle.capture` holds the last 64 KiB of output and is readable after exit.
- `SessionMetadata` is `deny_unknown_fields` with no `Default`. Its Cursor-only `cursor_chat_id`
  appears in about 50 struct literals across 36 files.
- `Harness` has 10 variants. `route_supported` refuses a preset per harness (Muse: Meta only), and
  the `PresetNotAccepted` Display hard-codes "it runs on Meta only".
- `worktree_removal_applies_to` lists `claude-cli | opencode-cli | muse-cli | workspace`.
- Web: `SessionType` has five members. The opencode/muse request branch sends `initialPrompt: ""`,
  `sandbox: false`, and so on. Create needs `projectId && model`.

### State B (Target)

- `CLI_HARNESSES: [CliHarness; 5]`. `WARP_CLI` = `{ session_type: "warp-cli", label: "Warp",
  route_harness: Harness::WarpCli, context_agent: "warp", default_binary: "warp", binary_env:
  "TDDY_WARP_CLI", route_config_env: None, argv: warp_argv, models: Curated(warp_cli_models) }`.
- `warp_argv`:
  - argv is `[binary]`, plus `["--resume", token]` when `launch.resume` and
    `conversation_id` is non-blank;
  - `model`, `initial_prompt` and the permission fields are ignored.
- `resume_token_from_output(bytes)`:
  - strip ANSI/OSC escape sequences and normalise `\r`;
  - find the **last** match of `warp --resume <TOKEN>`, where TOKEN is the next whitespace-delimited
    word;
  - `None` when there is no match.
- **Watcher**: `launch()` (or a sibling the two callers invoke) spawns a task. The task holds the
  handle's `pty_done` and `capture`; once `pty_done` is `true` it parses the capture. On `Some(token)`
  it reads `.session.yaml`, sets `harness_resume_token` and writes it back; on `None` it does nothing.
  It runs for every CLI harness launched through the generic path. Only Warp prints the line, so the
  other harnesses never record a token.
- **Resume**: the generic resume passes `conversation_id: meta.harness_resume_token.as_deref()`.
- `Harness::WarpCli` joins Cursor's `HarnessCannotBeRouted` arm, so the refusal reads "Warp cannot be
  pointed at another provider; it always runs on its own vendor". `route_on_account` checks
  `harness_routable` **before** its empty-model refusal. A `warp-cli` start naming
  `provider_account_id` is therefore refused `INVALID_ARGUMENT`, naming Warp, before anything is cut
  or spawned.
- A sandboxed `warp-cli` start is refused `FAILED_PRECONDITION` naming the runner ("the sandbox runner
  does not host warp-cli") **before** the jail route is resolved. Today the route is resolved first, so
  the refusal reads "needs a provider account".
- `context_globs_for_agent("warp")` = `AGENTS.md`, `.agents/**`, `WARP.md`.
- `worktree_removal_applies_to` includes `warp-cli`.
- Web: `SessionType` includes `"warp-cli"`, with toggle entry `{ id: "warp-cli", label: "Warp" }`.
  It shares the opencode/muse request branch with `model: ""`, shows no model field, and Create needs
  only `projectId`.

### Delta (What's Changing)

#### tddy-agent-backend
- New `src/cli_harness_warp.rs`, with `pub mod cli_harness_warp;` in `lib.rs`.
- `cli_harness.rs`: the registry grows to `[_; 5]`; the module doc names the fifth harness.
- `provider_route.rs`: the `WarpCli` variant and its arms. The `PresetNotAccepted` refusal text is
  per harness, so Warp's wording does not claim "Meta only".
- `backend/mod.rs`: the `"warp"` context-globs row.

#### tddy-agent-launch
- `opencode_cli_spawn.rs`: token-carrying resume, and the watcher attached after `launch()` at start
  and resume. The watcher's body goes in a new module if `opencode_cli_spawn.rs` would otherwise grow
  past its budget.
- `harness_profile_start.rs`: the `Harness::WarpCli` arm, which the compiler forces.

#### tddy-changeset
- `SessionMetadata.harness_resume_token`, with `#[serde(default, skip_serializing_if = "Option::is_none")]`.

#### tddy-session-activity
- `worktree_removal_applies_to` gains `"warp-cli"`.

#### tddy-web
- `createSessionRequest.ts`, `CreateSessionTypeToggle.tsx`, `CreateSessionPane.tsx`,
  `useCreateSessionSubmit.ts` (and `useCreateSessionModel.ts` if it must skip the model list).
- `cypress/support/testIds.ts`: `createSessionTypeWarpCliBtn`.

## Implementation Milestones

- [ ] Descriptor, argv, parser, route arm, context row: `warp_cli_acceptance.rs` green
- [ ] Metadata field plus literal sites; workspace `cargo check --all-targets` green (scoped crates listed in the commit)
- [ ] Watcher plus token-carrying resume: `warp_cli_session.rs` green
- [ ] Worktree removal: `worktree_removal_eligibility.rs` green
- [ ] Web: `CreateSessionWarpAcceptance.cy.tsx` green

## Testing Plan

### Testing Strategy

**Primary level: integration (daemon-level, real PTY, stub binary).** Token capture spans a real
spawned process, a real PTY, the capture ring and `.session.yaml` on disk. Only a daemon-level test
against a stub `warp` exercises the ordering that matters: the exit, then `pty_done`, then the
capture read, then the metadata write. The `opencode_cli_session.rs` harness (`CliHarnessDaemon`,
`a_stub_agent_script`, `read_last_appended_argv`) is the established shape. The new suite lives in
its own file, because that one is already 745 lines.

**Unit level** for the pure pieces: argv, the parser (escape codes, `\r`, several lines, none) and
route refusal. These run in `tddy-agent-backend/tests/warp_cli_acceptance.rs`, matching
`muse_cli_acceptance.rs`.

**Web**: one Cypress component spec with `mountWithRpc` + `anInMemoryRpcBackend`, modelled on
`CreateSessionMuseAcceptance.cy.tsx`.

### Stub `warp`

A `sh` stub records its argv (as the Muse stub does), then prints, with escape codes around it:

```
\e[2mResume this conversation with:\e[0m\r\n  \e[1mwarp --resume tok-123\e[0m\r\n
```

It then exits 0. Variants: one prints a different token per run (via an env var or a counter file),
and one prints nothing, exiting 0.

### Reliability

- Wait on observable state: `.session.yaml` holding the token, polled with the suite's `eventually`.
  Never sleep.
- The stub exits by itself, so `pty_done` is deterministic. No signal races.
- Tests are `#[serial_test::serial]` where they spawn PTYs, matching the sibling suites.

### Coverage

- Happy path: start, exit, token recorded, resume with the token.
- Edge cases: no token printed; a second exit replaces the token; a token wrapped in escape codes and
  `\r`.
- Errors: sandbox, managed codebase and provider account refused; a resume whose worktree is gone.
- Side effects: worktree removed on delete; no model recorded.

## Acceptance Tests

### tddy-agent-backend — `packages/tddy-agent-backend/tests/warp_cli_acceptance.rs`

Red state, verified 2026-10-24 (`cargo test -p tddy-agent-backend`): 7 of 9 fail on `todo!()`. `warp_is_registered_as_a_cli_harness` and `warp_accepts_no_provider_account` pass, because they assert published surface. The extended registry and route tests pass; `warp_reads_warp_md_on_top_of_the_shared_base` fails.

- [ ] **Unit**: `warp_is_registered_as_a_cli_harness` — `cli_harness("warp-cli")` has label "Warp", binary `warp`, env `TDDY_WARP_CLI`, context agent `warp`
- [ ] **Unit**: `a_warp_session_starts_with_no_arguments` — a fresh launch, even with a model and prompt set, gives `["warp"]`
- [ ] **Unit**: `a_resumed_warp_session_continues_its_recorded_conversation` — resume with `conversation_id = "tok-123"` gives `["warp", "--resume", "tok-123"]`
- [ ] **Unit**: `a_resumed_warp_session_without_a_token_starts_fresh` — resume with `None`, or a blank id, gives `["warp"]`
- [ ] **Unit**: `the_last_resume_line_on_exit_names_the_token` — two lines, the last wins
- [ ] **Unit**: `escape_codes_around_the_resume_line_are_ignored` — ANSI SGR plus `\r\n` gives the bare token
- [ ] **Unit**: `output_without_a_resume_line_names_no_token`
- [ ] **Unit**: `warp_accepts_no_provider_account` — `route_supported(WarpCli, p)` refuses every preset in `tddy_accounts` presets; the reason names the person's own Warp sign-in
- [ ] **Unit**: `warp_offers_no_models` — `warp_cli_models()` is empty, with an empty default
- [ ] **Unit (extended)**: `cli_harness_acceptance.rs` registry-order test → claude, cursor, opencode, muse, warp
- [ ] **Unit (extended)**: `provider_route_acceptance.rs` `every_harness_id_names_its_harness_and_back` includes `warp-cli`
- [ ] **Unit (extended)**: `context_globs_acceptance.rs` → a `warp` case: `AGENTS.md`, `.agents/**`, `WARP.md`

### tddy-session-lifecycle — `packages/tddy-session-lifecycle/tests/warp_cli_session.rs`

Red state, verified 2026-10-24: 12 of 13 fail. Most fail on the `warp_argv` `todo!()`. The sandbox and provider-account tests fail on today's refusal messages. The managed-codebase refusal already passes, on the generic CLI-harness check.

- [ ] **Integration**: `starting_a_warp_session_launches_warp_with_no_arguments`
- [ ] **Integration**: `a_warp_session_records_its_type_and_worktree_and_no_model`
- [ ] **Integration**: `a_warp_session_runs_in_its_own_worktree_on_a_warp_cli_branch`
- [ ] **Integration**: `a_warp_session_records_the_resume_token_warp_prints_on_exit`
- [ ] **Integration**: `a_later_exit_replaces_the_recorded_resume_token`
- [ ] **Integration**: `an_exit_that_prints_no_token_keeps_the_recorded_one`
- [ ] **Integration**: `resuming_a_warp_session_continues_its_recorded_conversation`
- [ ] **Integration**: `resuming_a_warp_session_with_no_recorded_token_starts_fresh`
- [ ] **Integration**: `resuming_a_warp_session_marks_it_active_again`
- [ ] **Integration**: `a_sandboxed_warp_session_is_refused`
- [ ] **Integration**: `a_warp_session_over_a_managed_codebase_is_refused`
- [ ] **Integration**: `a_warp_session_on_a_provider_account_is_refused`
- [ ] **Integration**: `a_warp_session_is_listed_with_its_agent`

### tddy-session-activity — `packages/tddy-session-activity/tests/worktree_removal_eligibility.rs`
- [ ] **Unit**: `a_warp_cli_session_is_eligible_for_worktree_removal`

### tddy-changeset — `packages/tddy-changeset/src/session_metadata.rs` (`#[cfg(test)]`)
- [ ] **Unit**: `a_session_without_a_resume_token_writes_no_resume_token_field`
- [ ] **Unit**: `a_recorded_resume_token_round_trips_through_session_yaml`

### tddy-web — `packages/tddy-web/cypress/component/CreateSessionWarpAcceptance.cy.tsx`

Red state, verified 2026-10-24: both fail. Create is disabled with no model, and the model/provider field renders.

- [ ] **Component**: "starts a warp-cli session with no model"
- [ ] **Component**: "offers no model or provider field for Warp"

## Technical Debt & Production Readiness

- [ ] **Ordering constraint for green**: the watcher must write `harness_resume_token` **before** the terminal is deregistered from `CliSessionManager`. `an_exit_that_prints_no_token_keeps_the_recorded_one` waits for deregistration and then asserts; it is deterministic only under that ordering. Alternatively, give the watcher an observable completion signal, and wait on that in the test instead.

## Decisions & Trade-offs

- **Token capture from the PTY tail, after exit** (developer, 2026-10-24). It is the only handle Warp
  exposes. Cost: a `warp` killed by a signal, or a daemon stopping first, records nothing, so the next
  resume starts a fresh conversation. Documented in the PRD.
- **A generic `harness_resume_token` field, not a Warp-specific one or a sidecar file** (developer
  approved the decomposition; recommendation taken). Any later harness that prints a resume handle
  reuses it. Cost: about 50 mechanical `None` literal edits, because `SessionMetadata` has no
  `Default`. A sidecar file would avoid that, but would split persistence across two places.
- **The watcher runs for every generic-path harness.** No per-descriptor flag: a harness that never
  prints the line records nothing. This keeps the descriptor shape unchanged.
- **Native Warp login only** (developer, 2026-10-24). The `warp` provider preset is deferred, because
  connecting an account probes an HTTP model endpoint (`provider_model_probe.rs:54`) and a Warp key
  has none.
- **No model field**: Warp has no `--model`, and the model is chosen in-session with `/model`.
- **Stack shape** (developer, 2026-10-24): two nodes, linear. This node is the root, with no real edge
  to node 2.

## Refactoring Needed

### From @red (TDD Red Phase)

- [ ] `tddy-session-lifecycle/tests/warp_cli_session.rs` duplicates about 90 lines of daemon fixtures (`daemon_config_with_cli_harness_binary`, `session_host`, `create_test_repo_with_origin`, `register_project`) from `opencode_cli_session.rs`. Extract a shared `tests/support/` module used by both suites.

### From @validate-changes (Change Validation)

### From @validate-tests (Test Quality)

### From @prod-ready (Production Readiness)

### From @analyze-clean-code (Code Quality)

## Validation Results

_(populated by validation commands)_

## Successor PRs

- `#warp-agent` 2/2: `feature/warp-agent/agent-terminal-tabs` (agent terminal tabs for any registered harness)

## TODO

- [x] Record initial discovery (`2026-10-24-warp-cli-session-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
