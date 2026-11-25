# Changeset: Sandbox accepts declared tools

**Date**: 2026-09-26
**Status**: 🚧 In Progress — requirements captured, red phase not started
**Type**: Feature

## Initial Discovery

[2026-09-26-sandbox-tool-specs-initial-discovery.md](./2026-09-26-sandbox-tool-specs-initial-discovery.md)
— three passes: the live `Grep` failure and the sandbox primitives, the `tddy-build` programmatic
API, and jail provisioning plus the Seatbelt/Linux renderers.

## Prerequisites

### ⛔ BLOCKING — PR #547 must land first

This change deletes `host_ripgrep_dir` and `FIXME(grep-in-jail)`, and closes
`docs/dev/todo/2026-09-26-grep-is-unreachable-inside-every-jail.md`. **None of those exist on
`master`** — they are on PR #547. Planning is against the shape #547 leaves.

Waiting is cheap here: this is a single PR with nothing stacked above it.

### ⚠ DURING — tddy-build's own deferrals — [`2026-06-16-tddy-build.md`](../todo/2026-06-16-tddy-build.md)

Two of the original changeset's deferrals land on this design:

- **"Output-publication convention — finalize the published-artifact layout … v1 stages under
  `.tddy-build/out/{target_id}/` only"** — unimplemented. A tool tree's location therefore comes
  from the target's declared `outputs`, and a target with none cannot be mounted (AC9).
- **"Cross-compilation architecture filter … for ToolTargets that ship per-arch binaries"** —
  anticipated, unbuilt, and exactly the `rg` case. **Recorded, not fixed here**: this change
  resolves a host binary on the host it runs on, so per-arch selection does not arise until a
  tool tree is shipped rather than resolved.

### ⚠ DURING — BUILD.yaml dependency edges are already stale — [`2026-09-23-…-nine-carved-crates.md`](../todo/2026-09-23-tddy-core-build-yaml-does-not-see-the-nine-carved-crates.md)

*"A tddy-build cache keyed on `tddy-core:lib`'s sources will not invalidate when any of that code
changes."* Edges are hand-declared and demonstrably incomplete.

This **tests the decision that the sandbox builds on demand** — that decision rests on the cache
knowing when a rebuild is required. It mostly misses this change: an external tool has its own
inputs, not the workspace crate graph. It lands on the **follow-up** (local build targets as tool
deps), where `tddy-tools:bin` could be served stale exactly as that entry describes.

**Recorded, not fixed here**, and named as a prerequisite of the follow-up.

### ⚠ DURING — five packages in this change's path have never been analyzed

`tddy-build`, `tddy-actions`, `tddy-sandbox`, `tddy-sandbox-recipes` and `tddy-sandbox-darwin`
have **no `docs/code-issues/` directory at all**. Not clean — unmeasured. Consider
`/analyze-code-issues` on the ones this change edits most.

`packages/tddy-daemon-sandbox/docs/code-issues/oversized-file-workspace-tool-sandbox.md` is open at
**660 production lines** against a 500 budget, and this change edits that file again.

### — Unrelated

No `Claimed by:` issue is in this change's path; the two repo-wide claims name
`tddy-index-daemon` and `tddy-workflow-recipes`.

## Affected Packages

- **tddy-sandbox**: the declaration→grant primitives; `runner_env.rs`'s hardcoded `PATH`
- **tddy-sandbox-darwin**: a `Literal` arm for exec in the Seatbelt renderer
- **tddy-sandbox-cgroups**: confirm the Linux side already honours it
- **tddy-sandbox-recipes**: `RunnerPlanRequest` has no `reads` field; decide whether to add one
- **tddy-daemon-sandbox**: `build_workspace_tool_plan`, the provisioner, deleting the stopgap
- **tddy-build** / **tddy-bsp**: programmatic build + `plugin_registry()`
- **tddy-session-lifecycle**: `<tddyhome>` onto the provisioner; the keyed build gate

## Related Feature Documentation

- [PRD-2026-09-26-sandbox-tool-specs.md](../../ft/daemon/1-WIP/PRD-2026-09-26-sandbox-tool-specs.md)
- [remote-codebase-mode.md](../../ft/daemon/remote-codebase-mode.md)
- [sandboxed-codebase-mode.md](../../ft/coder/sandboxed-codebase-mode.md)
- [tddy-build.md](../../ft/build/tddy-build.md)

## Summary

Give a jail a declared tool set. A `<tddyhome>/tools/*.yaml` entry names a host binary (pinned by
sha256) or a `tddy-build` target (built on demand), states what becomes readable, executable and
reachable on `PATH`, and may require others transitively. Resolution happens host-side before
spawn, in the one place all four jail entry points funnel through, and an unsatisfiable tool
refuses the jail by name.

## Background

See the PRD and the discovery document. In one line: `Grep` has never worked inside a sandboxed
session on a Mac, the fix shipped in #547 is a hardcoded special case for one binary, and almost
all the machinery for a general answer already exists in `tddy-build` and `tddy-actions` — except
a projection carrying read + exec + PATH, and any way for a session jail to ask for one.

## Scope

- [x] **Requirements captured**: PRD + changeset + discovery ✅
- [ ] **Naming settled**: the new concept is not `tool` — `tddy-build`'s `type: tool` already means
      a build-action `PATH` entry
- [ ] **Declaration + loader**: `<tddyhome>/tools/*.yaml`, refusing duplicates and unknown keys
- [ ] **Resolution**: transitive requires, cycle detection
- [ ] **Satisfaction**: host binary with checksum; build target via `tddy-build`, checking exit code
- [ ] **Projection**: into mounts / exec-marked reads / `exec_paths` / `PATH`
- [ ] **Renderer**: a `Literal` exec arm on macOS, and the platform contract stated
- [ ] **Wiring**: at `provision`, with a keyed build gate
- [ ] **First client**: `ripgrep`; delete `FIXME(grep-in-jail)`
- [ ] **Testing**: acceptance tests passing
- [ ] **Documentation**: feature docs + the macOS/Linux exec difference
- [ ] **Code Quality**: lint, format, review

## Technical Changes

### State A

Recorded in full in the discovery document. The four facts that define the work:

1. The jail's `PATH` is a literal at `packages/tddy-sandbox/src/runner_env.rs:12`; its exec surface
   is `shell_interactive_policy`'s six directories. `claude_interactive_policy` includes
   `/opt/homebrew`; the Shell recipe does not — the bug was recipe-specific.
2. `packages/tddy-sandbox-darwin/src/profile.rs:155-162` emits `process-exec*` only for
   `exec && kind == Subpath`, so a per-binary exec grant is **silently dropped** on macOS. Linux
   honours every kind and ignores `exec_paths` entirely.
3. `SandboxBuilder::build()` drops a read shadowed by an enclosing `Subpath`, including an
   `exec: true` child under a non-exec parent.
4. Four call sites reach `provision` → `build_workspace_tool_plan`. A resolve step anywhere else
   covers one of them.

### State B

Per the PRD. The load-bearing choices: resolution at `provision`; host-side materialisation only;
refusal rather than degradation; per-binary grants where the platform can express them.

### Delta

Deferred to the design step — the naming decision and the `RunnerPlanRequest`/`reads` question
change which crates carry what.

## Implementation Milestones

- [ ] M1 — Naming settled; declaration type + loader, duplicates and unknown keys refused
- [ ] M2 — Resolution: transitive requires, cycles refused
- [ ] M3 — Host-binary satisfaction with sha256 verification
- [ ] M4 — Build-target satisfaction, checking `exit_code`, refusing a target with no outputs
- [ ] M5 — Projection into the plan, surviving the shadowing rule
- [ ] M6 — Darwin `Literal` exec arm; platform contract stated and tested on both
- [ ] M7 — Wiring at `provision` + keyed build gate
- [ ] M8 — `ripgrep` declared; `FIXME(grep-in-jail)` deleted
- [ ] M9 — Documentation

## Testing Plan

Deferred to the red phase, with two constraints already known from the discovery:

- **The real-jail acceptance suite runs on no CI machine**
  ([`2026-09-12`](../todo/2026-09-12-the-in-jail-conversation-suite-runs-nowhere.md)), so the gate
  is unit and integration coverage plus the always-running relay suites.
- **Two silent-drop behaviours must be tested directly** — the Darwin `Subpath`-only exec guard and
  `build()`'s shadowing rule. Both fail by producing a plan that looks right and grants nothing,
  which is the shape of bug this whole change exists to remove.

## Acceptance Tests

Deferred to the red phase. AC1–AC20 in the PRD are the source.

## Technical Debt & Production Readiness

- [ ] Session start can block on a cold build; a broken `BUILD.yaml` becomes a session that will
      not start. Deliberate, and the alternative is a silent failure
- [ ] `tddy-build`'s `execute_target` returning `Ok` on a non-zero exit is worked around here, not
      fixed
- [ ] macOS and Linux disagree about what an exec grant is; this change states a contract but does
      not unify them

## Decisions & Trade-offs

- **`<tddyhome>/tools/*.yaml`, not `BUILD.yaml` target refs alone** — operator-editable without a
  repo change, mirroring `<tddyhome>/agents/*.yaml`. Cost: a second format, which must reference
  build targets to get caching.
- **The sandbox builds on demand** rather than consuming only what is cached. Cost: session start
  can block; mitigated by the content-addressed cache.
- **Built targets trusted, host binaries pinned by sha256.** A build output is content-addressed
  and reproducible in-tree; anything from the host is not, and a sandbox's exec surface is the
  wrong place to take that on trust.
- **`rg` is the first client** — it proves the mechanism on something already broken and lets this
  change delete the stopgap it replaces. Noted risk: if no other client lands, this is machinery
  for one tool that ~30 lines of the `regex` crate could have replaced. The node tooling is the
  real motivation and should arrive soon after.
- **`./desktop-dev`'s six runtime siblings are a follow-up**, with the stale-BUILD.yaml-edges entry
  as a named prerequisite.

## Refactoring Needed

### From /plan-red (Planning)

- [ ] `RunnerPlanRequest` has no `reads`/`extra_reads` field while `ProcessPlanRequest` does,
      forcing every caller into a mutate-after-build idiom
- [ ] `extra_read_specs` never calls `.executable()`, so an action's `extra_read_paths` grants read
      but not exec — and the PTY path ignores them entirely
- [ ] `load_agent_defs` does not detect duplicate names; `read_dir` order decides

## Validation Results

### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code
