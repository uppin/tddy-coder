# Changeset: Every jail runs a tool plan: the sandbox runner and recipes forget coding agents

**Created:** 2026-10-31
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 5 — `tddy-sandbox-app`, `tddy-session-split`, `tddy-sandbox-runner`, `tddy-sandbox-recipes`, `tddy-sandbox`, `tddy-sandbox-darwin`/`-cgroups`, `tddy-daemon-sandbox`; large deletions across a CLI surface; split suite must stay green unchanged
**Stack:** `#agent-tools 10/12` — branch `feature/agent-tools/jail-agnostic`, base `feature/agent-tools/headless-others`
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
**Greenable independently:** after #5, #7 and #8 are green (every agent's jail spec exists)
**Concurrent with:** #9 headless-others
**Blocks:** #12 agent-registry

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

### Unit / integration tests

- [ ] darwin `profile`: the context-dir write-deny and stdio rules come from `SandboxPlan` fields; argv no longer matters (a plan with an argv lacking `--context-dir` still renders the deny).

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
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
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
