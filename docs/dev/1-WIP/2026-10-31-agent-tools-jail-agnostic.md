# Changeset: Every jail runs a tool plan: the sandbox runner and recipes forget coding agents

**Created:** 2026-10-31
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 5 — `tddy-sandbox-app`, `tddy-session-split`, `tddy-sandbox-runner`, `tddy-sandbox-recipes`, `tddy-sandbox`, `tddy-sandbox-darwin`/`-cgroups`, `tddy-daemon-sandbox`; large deletions across a CLI surface; split suite must stay green unchanged
**Stack:** `#agent-tools 10/12` — branch `feature/agent-tools/jail-agnostic`, base `feature/agent-tools/headless-others`
**PR:** [#799](https://github.com/uppin/tddy-coder/pull/799)
**PRD:** [PRD-2026-10-31-tool-agnostic-jail.md](../../ft/coder/1-WIP/PRD-2026-10-31-tool-agnostic-jail.md)

## Initial Discovery

[2026-10-31-agent-tools-jail-agnostic-initial-discovery.md](./2026-10-31-agent-tools-jail-agnostic-initial-discovery.md)

State A below is distilled from that file. Grep traces and file dumps are not repeated here.

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-10-22-sandbox-app-cannot-start-opencode-or-muse.md](../todo/2026-10-22-sandbox-app-cannot-start-opencode-or-muse.md) | ⚠ During — **narrowed at wrap**. The app takes `--agent <spec id>`, so the missing per-agent binary flags are answered by spec overrides. The route requirement remains and stays the entry's subject |
| `packages/tddy-sandbox-runner/docs/code-issues/oversized-file-runner.md` | ⚠ During. The agent modes are deleted. Re-measure at wrap, and promote to ✅ if under budget |
| `packages/tddy-sandbox/docs/code-issues/oversized-file-builder.md` | ⚠ During. `allow_oauth_inbound` stays a field; no growth |
| [2026-10-01-the-sandbox-runner-links-git-code-through-a-types-re-export.md](../todo/2026-10-01-the-sandbox-runner-links-git-code-through-a-types-re-export.md) | — Unrelated (dependency shape, not agent modes) |

## Affected Packages

- [ ] [`tddy-sandbox-app`](../../../packages/tddy-sandbox-app/README.md): `--agent <spec id>` replaces `--agent-kind`; `AgentKind` and the duplicate home seeding are removed; launches through spec rendering plus `--exec-plan`
- [ ] [`tddy-session-split`](../../../packages/tddy-session-split/README.md): the host agent and workspace jail render from `claude.yaml` (a declared host-agent variant)
- [ ] [`tddy-sandbox-runner`](../../../packages/tddy-sandbox-runner/README.md): agent flags and modes deleted
- [ ] [`tddy-sandbox-recipes`](../../../packages/tddy-sandbox-recipes/README.md): agent arms, `detect_recipe_from_argv`, `claude_runner_env_overlay`, the agent credential and MCP writers deleted
- [ ] [`tddy-sandbox`](../../../packages/tddy-sandbox/README.md): `MANAGED_CODEBASE_PREAMBLE`/`PREAMBLE_FILES`/`workspace_exec_tool_names` move out; `allow_oauth_inbound` is only tool-set-derived
- [ ] [`tddy-sandbox-darwin`](../../../packages/tddy-sandbox-darwin/README.md): the context dir and stdio come from the plan, not runner argv
- [ ] [`tddy-sandbox-cgroups`](../../../packages/tddy-sandbox-cgroups/README.md): the same
- [ ] [`tddy-daemon-sandbox`](../../../packages/tddy-daemon-sandbox/README.md): `build_allow_read_paths` argv sniffing deleted

## Responsibility

- `tddy-sandbox-app --agent <id>`. It loads the registry, including `<tddyhome>/tools` overrides, and renders and launches through the same functions the daemon uses. An unknown id is refused, naming the registry ids.
- The split session host agent is a `coding_agent.variants.host_agent` in `claude.yaml` (`--strict-mcp-config --setting-sources user`, the native disallow list from the slot).
- `SandboxPlan.{context_dir: Option<PathBuf>, stdio: bool}`, read by the darwin and cgroups backends instead of argv.
- Deletions:
  - in the runner: `--agent-kind`, `--claude-binary`, `--agent-binary`, `--claude-arg`, `--agent-arg`, `--permission-mode`, `--dangerously-skip-permissions`, `--resume`, `--append-system-prompt-file`, `--mcp-log-level`, `spawn_claude_pty` and `spawn_cursor_pty`;
  - in the recipes: `SandboxRecipe::{ClaudeCli, CursorCli, OpenCodeCli, MuseCli}`, `detect_recipe_from_argv`, `claude_runner_env_overlay` and `prepare_persistent_claude_home` (both copies).
- `tddy_sandbox` contains no agent identifier, pinned by a source-scan test.

## Boundaries

- No provider route for the standalone app.
- No change to agent behaviour. Every agent already launches from specs on the daemon path.
- No new runner capability.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n5` claude-jail | `launch_jailed`'s rendering, `--exec-plan`, homes | the app and split render and launch the same way | change the launcher contract |
| `n7` cursor-cli | `cursor.yaml` (jail) | `--agent cursor` in the app | change the spec |
| `n8` harness-clis | `opencode.yaml`, `muse.yaml`, `placements` | `--agent opencode|muse` (still refused without a route) | change the specs |
| `n2` claude-host | slots and `claude.yaml` | adds the `host_agent` variant | change existing slots |

## Draft PR contract

The first push publishes `SandboxPlan.{context_dir, stdio}`, the app's `--agent` flag, the `host_agent` variant, the source-scan test, and the failing tests below. The deletions happen in green, once every consumer is moved. Unimplemented bodies are `// TODO(jail-agnostic): implement`.

## Green wave

**Wave:** 5 of 7
**Greenable independently:** after #794, #796 and #797 are green (every agent's jail spec exists)
**Concurrent with:** #798 headless-others
**Blocks:** #801 agent-registry

Contract-stage finding: the app's `choose_agent`/`agent_exec_plan` consume `n1` (`resolve_closure`, `ProcessToolHost`) and `n2` (`load_agent_registry`, `CodingAgentSpec::of`, `slots::render`, `render_argv`) directly, and `n7`'s placed MCP slot for `--agent cursor`. All three are already upstream of this node through `n5`/`n7`, so the wave is unchanged.

Real dependency edges, as opposed to the branch line:

    n1 → n2, n3, n4      n2 → n5, n6      n3 → n5, n7, n8      n5 → n7, n8, n10
    n6 → n9      n7 → n9, n10      n8 → n10, n11      n9 → n11      n2..n11 → n12

## State A

- `tddy-sandbox-app` `spawn.rs` has `AgentKind` (Claude, Cursor, OpenCode, Muse), duplicate `seed_claude_credentials` and `seed_claude_local_install`, and `host_home=$HOME`.
- `tddy-session-split` `agent_argv.rs` and `split_session.rs` build Claude argv via recipes (claude-cli only).
- The runner and recipes carry agent modes and arms. The recipe is inferred by sniffing argv in three layers.
- `tddy-sandbox` carries `MANAGED_CODEBASE_PREAMBLE`, `PREAMBLE_FILES` and `workspace_exec_tool_names`.

## State B

- Every jail is a tool plan.
- The runner execs `--exec-plan` or serves `--workspace-tools`, and knows no agent.
- The recipes know only generic shapes (`Generic`, `Shell`, `RunnerPty`) plus tool lowering.

## Delta

### New
- `packages/tddy-sandbox/tests/no_agent_vocabulary.rs` (source scan)
### Modified
- `tddy-sandbox-app` `main.rs`, `spawn.rs`, `host_agent.rs`; `tddy-session-split` `agent_argv.rs`, `split_session.rs`.
- darwin `profile.rs`, cgroups spawn; `tddy-daemon-sandbox` `sandbox_session.rs`.
- Docs at wrap: `docs/ft/coder/sandbox-builder.md`, `sandboxed-codebase-mode.md`; `packages/tddy-sandbox*/docs/architecture.md`; `packages/tddy-sandbox-app/README.md`.
### Removed
- `packages/tddy-sandbox-recipes/src/{claude_cli.rs, cursor_cli.rs, opencode_cli.rs, muse_cli.rs}` (the agent parts). Their generic remnants move into `tool_grants`
- The runner's agent modes

## Testing Strategy

### Acceptance tests

**`tddy-sandbox-app`: `packages/tddy-sandbox-app/tests/agent_from_spec_acceptance.rs`**

- [ ] `sandbox_app_agent_claude_starts_a_jail_from_the_spec_with_todays_behaviour` (macOS, stub claude)
- [ ] `sandbox_app_agent_cursor_does_the_same`
- [ ] `sandbox_app_agent_opencode_is_refused_without_a_route_as_today`
- [ ] `an_unknown_agent_id_is_refused_naming_the_registry_ids`

**`tddy-session-split`: the existing split acceptance suite**

- [ ] passes unchanged, with the host agent and workspace jail started from specs (asserted by the recorded argv)

**`tddy-sandbox-runner`: `packages/tddy-sandbox-runner/tests/runner_cli_surface.rs`**

- [ ] `the_runner_help_lists_no_agent_flag`
- [ ] `passing_agent_kind_is_an_error`

**`tddy-sandbox`: `packages/tddy-sandbox/tests/no_agent_vocabulary.rs`**

- [ ] `tddy_sandbox_and_tddy_sandbox_recipes_name_no_coding_agent`
- [x] `the_scan_finds_an_agent_named_in_any_casing` (positive control for the scanner; passes)

**`tddy-coding-agent-specs`: `packages/tddy-coding-agent-specs/tests/host_agent_variant.rs`** (added at contract)

- [ ] `the_claude_host_agent_variant_loads_no_checkout_config_and_withdraws_every_native_tool`
- [ ] `a_variant_the_agent_does_not_declare_is_refused_naming_the_ones_it_does`
- [ ] `a_variant_that_withdraws_tools_through_an_unsupported_deny_slot_is_refused`

### Unit / integration tests

- [ ] darwin `profile` (`src/profile.rs` tests): `rendered_profile_denies_writes_under_the_plans_context_dir_whose_argv_names_none`, `rendered_profile_ignores_a_context_dir_flag_the_plan_does_not_declare`.
- [ ] darwin `spawn` (`src/spawn.rs` tests): `a_plan_marked_stdio_is_spawned_in_stdio_mode_without_the_flag_in_its_argv`, `a_plan_not_marked_stdio_is_not_spawned_in_stdio_mode_whatever_its_argv_says`.
- [ ] cgroups (`src/lib.rs` tests, Linux only — compiled and run on CI): the same two stdio tests.

### Test level decisions

| Aspect | Level | Rationale |
|---|---|---|
| App and split launch | Integration with stubs, live on macOS | Behaviour-preserving |
| No-vocabulary claim | Source scan | A deletion claim is only durable if pinned |

## Decisions & Trade-offs

- **Deletions only after every consumer moves.** Commit 2 publishes the new surface and tests, and green moves the consumers and deletes in that order.
- **A source scan pins agent-agnosticism**, like the existing stdout scan. It is a cheap and permanent guard.

## TODO

- [x] Record initial discovery (`2026-10-31-agent-tools-jail-agnostic-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests — waived by the developer 2026-10-31: pass 2 runs without per-node review
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

## Refactoring Needed
### From /red (TDD red phase)

**Published surface (commit 2).**

- `tddy_sandbox::SandboxPlan.{context_dir: Option<PathBuf>, stdio: bool}` and the matching `SandboxBuilder::{context_dir, stdio}` setters (real: they only carry the value). Nothing reads either field yet.
- darwin `profile::context_dir_of(plan)` and `spawn::stdio_mode_of(plan)`, and cgroups `stdio_mode_of(plan)`. Each still returns today's argv sniffing, under `TODO(jail-agnostic)`; green makes them return the plan field. No jail loses its carve-out or its stdio transport at contract stage.
- `tddy_coding_agent_specs::{AgentVariant, VariantError, HOST_AGENT_VARIANT}`, `CodingAgentSection.variants` (additive, `#[serde(default)]`) and `CodingAgentSpec::variant_args` (`todo!`).
- `claude.yaml` gained `coding_agent.variants.host_agent`: `args: [--strict-mcp-config, --setting-sources, user]`, and `withdraw_native` with the split session's 11 natives. This is a new key only; no existing section changed.
- `tddy_sandbox_app::agent_spec::{ChosenAgent, AgentJail, AgentChoiceError, TOOLS_SUBDIR, choose_agent, agent_exec_plan}`. `choose_agent` and `agent_exec_plan` are `todo!`. The app's manifest gained `tddy-actions` and `tddy-coding-agent-specs`, both path crates.
- `tddy-sandbox-app --agent <id>`, which conflicts with `--agent-kind`. It reaches `choose_agent` and then a `todo!` launch on macOS, and a `todo!` on Linux. The flag is new, so no current invocation reaches either.
- `tddy-session-split` `agent_argv.rs`: one `TODO(jail-agnostic)` comment and no code change.

**Red baseline.** Scoped `./dev cargo test -p tddy-sandbox -p tddy-sandbox-darwin -p tddy-sandbox-runner -p tddy-coding-agent-specs -p tddy-sandbox-app --no-fail-fast`. Every new test fails because of this node:

- `host_agent_variant.rs` (3) and `agent_from_spec_acceptance.rs` (4) fail on `todo!("jail-agnostic: …")`.
- `no_agent_vocabulary.rs` fails on its assertion. The mentions live in `claude_cli.rs`, `cursor_cli.rs`, `opencode_cli.rs`, `muse_cli.rs`, `plan.rs`, `route_files.rs`, `context_dir.rs`, `links.rs`, `walk.rs`, `context_manifest.rs` and `lib.rs`. These are the specification for green.
- `runner_cli_surface.rs` (2) fails on its assertions: the flags are still listed, and `--agent-kind` still parses (the run fails on missing required args instead).
- The 4 darwin unit tests fail on their assertions, because the argv is still read.

The cgroups tests are `#[cfg(target_os = "linux")]`. They are not compiled on this Mac, and CI runs them.

**Pre-existing / parent-owned failures in the touched packages** (not this node):

- `tddy-coding-agent-specs`: every suite except the new one fails on parent `todo!`s (`claude-host`, `cursor-cli`, `harness-clis`, `headless-others`, `claude-headless`).
- `tddy-sandbox`: 9 `network::tests` fail on `egress-allowlist` `todo!`s.
- `tddy-sandbox-runner`:
  - 3 `exec_plan` tests fail on `claude-jail`;
  - 3 egress-shim tests plus `egress_allowlist_relay_acceptance` (3) fail on `egress-allowlist`;
  - `hook_report_relay` (3) fails on `cursor-cli`.
- `tddy-sandbox-darwin`: `sandbox_runner_behavior_acceptance::sandbox_runner_streams_demo_tui_dimensions_on_session_channel` fails for an environment reason: `tddy-demo-tui` is not built.

Every other existing test in these packages passes, the seatbelt acceptance suites included. Scoped clippy (`-D warnings`, `--all-targets`, the touched packages plus `tddy-sandbox-cgroups` and `tddy-session-split`) and `cargo fmt --all --check` are clean.

**Deviations from the plan.**

- The app tests pin the **`ExecPlan` the app hands the runner**, not a live jail. A live jail would fail first on `run_exec_plan` (`TODO(claude-jail)`), which is not attributable to this node. The tests are therefore not `#[cfg(target_os = "macos")]`: the plan is platform-free, so they run on CI too. The stub binaries are named through the specs' env overrides (`TDDY_CLAUDE_BINARY`, `TDDY_CURSOR_AGENT`, `#[serial]`).
- Added `host_agent_variant.rs`, so the published variant has a test. It reads the embedded specs with serde, bypassing the registry, so it does not wait on `claude-host`.
- Added the converse tests: the argv alone no longer declares a context dir or switches stdio. Added a stdio pair per backend.
- The split acceptance suite is untouched; green must keep it passing unchanged.
- No code-issue record is ✅ RESOLVED HERE, so no claims were written.

**For green (and the developer's decisions).**

1. **Deletions need the developer's approval** (CLAUDE.md: ask before deleting files). The deletions:
   - files: `tddy-sandbox-recipes/src/{claude_cli,cursor_cli,opencode_cli,muse_cli}.rs` and `tddy-sandbox-runner/tests/agent_kind_opencode_muse.rs`;
   - code:
     - the runner's agent flags (the `runner_cli_surface.rs` list), `spawn_claude_pty` and `spawn_cursor_pty`;
     - `SandboxRecipe::{ClaudeCli, CursorCli, OpenCodeCli, MuseCli}`, `detect_recipe_from_argv` and `claude_runner_env_overlay`;
     - both `prepare_persistent_claude_home`s;
     - the app's `AgentKind`, `--agent-kind`/`--claude-binary`/`--cursor-binary`, and `seed_claude_credentials`/`seed_claude_local_install`/`seed_cursor_credentials`;
     - darwin's `context_dir_from_runner_argv`;
     - `tddy-daemon-sandbox` `build_allow_read_paths` argv sniffing.
   - `MANAGED_CODEBASE_PREAMBLE`, `PREAMBLE_FILES` and `workspace_exec_tool_names` move out of `tddy-sandbox`, along with the context-glob comments naming `CLAUDE.md`/`.cursor`, which the scan also catches.
2. **Existing tests green must update, not delete**:
   - darwin's three `a_runner_plan_told_its_context_dir` tests: the fixture must set `plan.context_dir`;
   - cgroups' `stdio_piping_acceptance`: it must set `stdio: true`;
   - every launcher that builds a plan (the daemon's plan builders and `tddy-sandbox-app`) must set both fields.
3. **Host-agent variant: a decision for the developer.**
   - The variant follows this changeset: `--strict-mcp-config --setting-sources user` plus the 11 natives of `tddy-session-split`'s `NATIVE_FILESYSTEM_TOOLS`.
   - A split session today does **not** pass `--setting-sources user`. Moving split onto the variant therefore adds it, which is a behaviour change against "No change to agent behaviour".
   - The app's own host agent (`host_agent.rs`, sandboxed-codebase mode) withdraws 10 natives, without `LS`.
   - Either accept the variant as the one host-agent shape, or split it into two variants.
4. `cgroups` has no context-dir carve-out today, unlike Seatbelt. Reading `plan.context_dir` there means **adding** a read-only bind, which is a new capability against "No new runner capability". Green should confirm the scope with the developer.
5. `choose_agent`'s `known` list is the registry's coding agents, sorted. `warp` is host-only (`placements: [host]`), so green also refuses `--agent warp` through `require_placement`.
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
| Validation | Last Run | Status |
|------------|----------|--------|
| /validate-changes | Not yet run | Pending |
| /validate-tests | Not yet run | Pending |
| /validate-prod-ready | Not yet run | Pending |
| /analyze-clean-code | Not yet run | Pending |
