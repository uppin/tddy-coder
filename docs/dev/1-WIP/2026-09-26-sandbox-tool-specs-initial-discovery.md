# Initial Discovery: Sandbox accepts declared tools

**Changeset**: [2026-09-26-sandbox-tool-specs.md](./2026-09-26-sandbox-tool-specs.md)
**Date**: 2026-09-26
**Passes**: 3

## Combined Conclusions

### What this is

A jail's external tooling is declared nowhere. `Grep` shells out to `ripgrep`; the jail's PATH is
`/usr/bin:/bin:/usr/sbin:/sbin` and `shell_interactive_policy`'s `exec_paths` lists six system
directories, so `Grep` has never worked inside a sandboxed session on a Mac. PR #547 patches it by
resolving the host's `rg` and granting its directory, and says in three places that it is a
stopgap.

This change replaces that with a declared list: a jail states the tools it needs, each declaration
states what becomes readable, executable and reachable on `PATH`, and a tool backed by a build
target is built on demand.

### The single most important finding: most of this exists, but **not** the part that shares its name

`tddy-build` is a Bazel-inspired, content-addressed build system with a DAG, an action cache and
per-ecosystem plugins. **45 `BUILD.yaml` files are live** in this repo. So resolution, caching and
per-ecosystem packaging are solved.

**But `tddy-build`'s existing `type: tool` is not the thing this change needs**, and the collision
of names is a trap:

```yaml
- id: "tools:bin"
  config:
    type: tool
    bin_dir: tools/bin
    commands: { stamp: stamp }
```

`ToolTarget` (`packages/tddy-build/src/builtin.rs:37-41`) has **no output and produces no action**
(`builtin.rs:73-77`, `executor.rs:266-275`). It contributes a `bin_dir` to a dependent *build
action's* `PATH`, via `tool_dep_ids` on the action, and assumes the tool already exists there. Its
`commands` field is `#[allow(dead_code)]` — declared and never wired.

What this change needs is different: **a tree mounted into a jail**, with read, exec and `PATH`
grants. Two concepts, one word. Naming them apart is a design decision, not a detail.

### The gap, precisely

`tddy-actions` already declares jail semantics — `ActionInput { host_path, jail_path, writable }`
and `SandboxRequest { output_dir, extra_read_paths, recipe, stdin }` — and
`tddy-daemon-sandbox/src/sandbox_plan_builder.rs:44` `input_mounts()` turns those into
`MountSpec { host, jail, writable }`.

**Neither `tddy-actions` nor `tddy-build` mentions `executable()` or `exec_paths` anywhere** —
grepped both, zero hits. Meanwhile `tddy-sandbox` has exactly the primitives needed:
`ReadSpec::subpath(..).executable()`, `PolicySpec.exec_paths`, `MountSpec`, `CopySpec`,
`SymlinkSpec`. So the missing piece is a projection carrying **read + exec + PATH** across the
action/sandbox boundary, plus the wiring that lets a *session* jail ask for it at all —
`build_workspace_tool_plan` knows nothing about `tddy-build` today.

### Decisions taken before planning

1. **Declarations live in `<tddyhome>/tools/*.yaml`**, mirroring `<tddyhome>/agents/*.yaml`. An
   entry names either a build target id or a host path.
2. **The sandbox invokes `tddy-build` when a dep is a build target needing rebuild.** Not
   cache-only.
3. **Trust**: a built target is trusted because the build is content-addressed; a **host** tool
   carries a sha256 and a mismatch refuses the jail.
4. **`rg` is the first client**, and this change deletes `FIXME(grep-in-jail)`.
5. **`./desktop-dev`'s six runtime siblings are a follow-up**, not this PR.

### What the API actually offers, and its four sharp edges

The pattern to copy is `tddy-vm`'s `build_vm_image` (`packages/tddy-vm/src/build.rs:1231-1274`):
discover manifests → `BuildGraph::from_manifests` → `tddy_bsp::plugin_registry()` → read output
paths from `graph.actions_for(id)` **before** executing → `execute_target`.

| Edge | Consequence for this change |
|---|---|
| **`execute_target` returns `Ok` on a non-zero exit** (`executor.rs:88-92`) — nothing checks it, and `service.rs:87` injects `"status":"ok"` unconditionally | a failed tool build would silently produce an unusable jail. The caller **must** inspect `record.actions[*].exit_code` |
| **No "is it up to date?" API.** `fingerprint_inputs` is private (`executor.rs:285`); `dry_run` skips the cache entirely | either make it `pub`, or use the honest proxy: after `execute_target`, every `ActionOutcome.cached == true` means nothing was rebuilt |
| **No output staging.** `.tddy-build/out/{target_id}/` is explicitly unimplemented (`docs/ft/build/tddy-build.md:172`); outputs land wherever the tool wrote them, author-declared and repo-root-relative | a tool tree's location comes from the target's declared `outputs`, so a target with none cannot be mounted |
| **Ids are opaque strings**, no label type, no `//path:name` syntax, no directory-relative resolution | a tool→target reference is a `String` validated against `graph.target(id)` |

### Two Step 2b items that bear on the decisions

- ⚠ [`2026-06-16-tddy-build.md`](../todo/2026-06-16-tddy-build.md) — the original changeset's own
  deferrals. Two land here: the **output-publication convention** is unfinished (the location
  problem above), and a **cross-compilation architecture filter** is already anticipated "for
  ToolTargets that ship per-arch binaries" — exactly the `rg` case.
- ⚠ [`2026-09-23-…-build-yaml-does-not-see-the-nine-carved-crates.md`](../todo/2026-09-23-tddy-core-build-yaml-does-not-see-the-nine-carved-crates.md)
  — `BUILD.yaml` dependency edges are hand-declared and **already demonstrably stale**: "a
  tddy-build cache keyed on `tddy-core:lib`'s sources will not invalidate when any of that code
  changes."

  **This tests decision 2 and mostly misses this PR.** An external tool (`//tools/ripgrep`) has its
  own inputs, not the workspace crate graph. It lands squarely on the **follow-up**, where
  `tddy-tools:bin` as a tool dep could be served stale exactly as that entry describes. That is a
  real prerequisite for the follow-up and an argument for the scope split.

### Coverage note for the follow-up

`tddy-tools:bin` **does** declare `outputs: target/debug/tddy-tools`, so the follow-up's premise
holds for it. Many rust targets omit `outputs:` entirely, so each of the six siblings needs
checking before it can be a tool dep.

## Exploration 1: Where the idea came from — 2026-09-26

**Agent**: parent, during a live `./desktop-dev` debugging session
**Scope**: the `Grep` failure in session `01a0deca-…-b1e20272b4d9`, and the sandbox primitives.

Findings recorded in full in
[`plans/sandbox-toolspec-brief.md`](../../../plans/sandbox-toolspec-brief.md) and
`docs/dev/todo/2026-09-26-grep-is-unreachable-inside-every-jail.md` (the latter exists only on
PR #547). In summary: `Grep` returned `spawn failed: No such file or directory` on every call;
`rg` is at `/opt/homebrew/bin/rg`; the jail PATH and `exec_paths` exclude it; the jail has **no
network** (`loopback_allow_ports: vec![]`, no egress shim), so nothing can install inside it and
every tool tree must be materialised host-side before spawn.

## Exploration 2: The tddy-build programmatic API — 2026-09-26

**Agent**: Explore subagent
**Scope**: `tddy-build` manifest schema, service/executor/cache/graph/plugin APIs, outputs, callers.

Full findings are folded into Combined Conclusions above. The load-bearing specifics:

- `BuildTarget { id, name, deps, tags, languages, capabilities, actions, config }` —
  `packages/tddy-build/src/manifest.rs:23-47`; `config` is an open `{ type, ..fields }` map
  (`:59-67`).
- Builtin types are only three: `script`, `tool`, `group` (`builtin.rs:15-25`). Everything else is
  a plugin: `rust_binary`/`rust_library`, `typescript`, `docker_image`, `buildroot_image`,
  `qemu_disk_image`.
- `typescript` already emits `OutputKind::Directory` for `output_dirs` — the only existing target
  type whose output is a **tree** (`packages/tddy-build-typescript/src/lib.rs:23,40-47`).
- One plugin assembly point: `tddy_bsp::plugin_registry()` (`packages/tddy-bsp/src/plugins.rs:9-17`).
  Reuse it; `tddy-vm` hand-rolls a QEMU-only registry and should not be copied in that respect.
- `build_action_to_spec` (`packages/tddy-build/src/action_convert.rs:17`) converts one way only,
  `BuildAction → tddy_actions::ActionSpec`, and `packages/tddy-bsp/src/build_cli.rs:24-26` notes
  those specs' absolute input paths "become jail mounts" — the existing seam closest to this work.

## Exploration 3: Jail provisioning, the yaml pattern, and the Seatbelt renderer — 2026-09-26

**Agent**: Explore subagent
**Scope**: `<tddyhome>/agents/*.yaml` loading; every type a `SandboxPlan` is made of; the Darwin
and Linux renderers; the full `StartSession` → `provision` trace.

### ⛔ The headline benefit is not expressible today

`docs/.../profile.rs` renders `process-exec*` from exactly four sources
(`packages/tddy-sandbox-darwin/src/profile.rs:149-170`): `project_root`, `policy.exec_paths`,
`plan.mounts` (**every** mount, writable or not), and

```rust
for r in &plan.reads {
    if r.exec && r.kind == ReadKind::Subpath {
```

So **`ReadSpec::literal(bin).executable()` produces no exec entry at all on macOS** — the
`kind == Subpath` guard drops it silently. "The grant is the binary rather than its directory",
the improvement this change sells over the monkeypatch, needs a new `Literal` arm in the renderer.

Linux already does it right: `packages/tddy-sandbox-cgroups/src/lib.rs:37-63` honours `exec` on
every read kind via `MS_NOEXEC`, and **ignores `policy.exec_paths` entirely**. So the six-directory
`shell_interactive_policy` list buys nothing there, and the two platforms disagree about what an
exec grant even is.

Three code sites a per-binary grant touches:
1. `packages/tddy-sandbox-darwin/src/profile.rs:155-162` — the `Subpath`-only guard.
2. `packages/tddy-sandbox/src/runner_env.rs:12` — the hardcoded `PATH` literal, the single point
   every in-jail `PATH` flows from.
3. `packages/tddy-sandbox-recipes/src/plan.rs:195-214` — `shell_interactive_policy`'s six
   directories.

### A second trap in `SandboxBuilder::build()`

`packages/tddy-sandbox/src/builder.rs:458-473` drops a read shadowed by an enclosing `Subpath`
read — **including an `exec: true` child under a non-exec parent**. A per-binary exec grant inside
an already-granted tree would be silently lost. Any projection must be tested against this.

Also: `build()` unconditionally resets `plan.cgroup` (`:416-550`), which is why
`build_workspace_tool_plan` sets it after the call.

### Where the resolve step goes — decided by call-site arithmetic

**Four** call sites funnel into `provision` → `build_workspace_tool_plan`:
`start_session_core` (`svc_start_session_core.rs:287-292`), resume
(`session_coordinate_handlers/svc_resume_session.rs:78-88`), jailed-codebase reprovision
(`svc_start_sandboxed_codebase_session.rs:169-183`), and the mid-session rebuild
(`jail_relaunch.rs:126-129`).

A resolve step in **`JailedWorkspaceSandboxProvisioner::provision`**
(`workspace_tool_sandbox.rs:281-299`, just before the plan call) is covered by all four for free.
One in `start_session_core` covers **one**, and would leave every resume and every rebuild
building a plan with a different tool set — a silent divergence of exactly the kind this change
exists to remove.

Two things that seam lacks and must be threaded in:

- **`<tddyhome>`.** `JailedWorkspaceSandboxProvisioner` is a *unit struct* with no fields
  (`workspace_tool_sandbox.rs:276-277`), constructed at
  `svc_resolve_tddy_tools_path/svc_host_builders.rs:81-83`. Reading `<tddyhome>/tools/*.yaml`
  needs a field there, or a field on `WorkspaceSandboxSpec` (built in one place,
  `jail_relaunch.rs:32-44`).
- **Concurrency.** Two sessions needing the same tool must not build it twice. The pattern to copy
  is `JailRelaunch`'s keyed gate (`jail_relaunch.rs:51-59`, `gate_for` `:141-149`,
  `forget_unused_gate` `:158-166`) — built earlier today for the same class of problem.

Failure discipline is already stated at `svc_start_session_core.rs:293-296`: *"a session surviving
a start that answered with an error is one the operator can see, list and resume, whose tools were
never confined."* An unsatisfiable tool must fail **there**.

### The `<tddyhome>/*.yaml` pattern, and the bug to not copy

`load_agent_defs` (`packages/tddy-discovery/src/agent_def.rs:168-195`): a missing directory returns
empty with no log; an unreadable or malformed file is a `log::warn!` and a skip, so the rest of the
directory still loads; `deny_unknown_fields` rejects a typo'd key.

**Duplicate `name` across two files is not detected.** Both are pushed, and every caller does
`.find(|d| d.name == …)` — so first-seen in `read_dir` order wins, and that order is OS-dependent.
A `tools/` copy should refuse a duplicate rather than inherit this.

Also worth copying: the hand-written `Debug` that redacts `api_key` (`agent_def.rs:145-162`). A
derived `Debug` would leak it. Relevant if a tool declaration ever carries a credential.

No caching anywhere — every call re-reads the directory, deliberately
(`packages/tddy-model-registry/src/store.rs:92-94`): *"a def written after this daemon started is
resolvable the moment the file lands."*

Precedence, for a tools/ analogue to mirror or deliberately not: the registry wins a name tie over
a YAML def (`svc_resolve_listed_worktree.rs:403-418`), and the write side refuses to create an
assistant whose name a YAML def already owns (`store.rs:672-680`).

### Smaller facts that shape the API

- `RunnerPlanRequest` has **no `reads`/`extra_reads` field** (`plan.rs:65-77`), while
  `ProcessPlanRequest` does (`:91`). That asymmetry is why `build_workspace_tool_plan` mutates
  `plan.reads` after the call, and why a tool projection will too.
- `sandbox_plan_builder.rs:100-105` `extra_read_specs` never calls `.executable()`, so an action
  declaring a tool directory in `extra_read_paths` gets read but not exec.
- The PTY action path ignores `extra_read_paths` entirely — `RunnerPlanRequest` cannot carry them.
- **`claude_interactive_policy()` already includes `/opt/homebrew` and `/opt/local`**
  (`packages/tddy-sandbox-recipes/src/claude_cli.rs:129-152`). The Claude recipe would have found
  `rg`; the Shell recipe the workspace jail uses would not. The bug was recipe-specific.
- Every `plan.mounts` entry is exec, writable or not, with no way to say otherwise.
