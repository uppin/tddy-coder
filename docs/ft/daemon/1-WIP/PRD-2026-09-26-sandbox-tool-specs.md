# Sandbox accepts declared tools - PRD

**Date**: 2026-09-26
**PRD Type**: Enhancement

## Affected Features

- **Primary**: [Remote codebase mode](../remote-codebase-mode.md) § *Workspace tool sandbox* — the
  jail gains a declared tool set; `Grep` stops being unreachable inside it
- **[Sandboxed codebase mode](../../coder/sandboxed-codebase-mode.md)** — what a jail may execute
  becomes a declaration rather than a hardcoded list
- **[tddy-build](../../build/tddy-build.md)** — a build target becomes something a jail can consume

## Summary

A jail's external tooling is declared nowhere. The jail's `PATH` is a string literal
(`tddy-sandbox/src/runner_env.rs:12`) and its executable surface is six hardcoded directories
(`shell_interactive_policy`), so `Grep` — which shells out to `ripgrep` — has never worked inside
a sandboxed session on a Mac. PR #547 patches that by resolving the host's `rg` and granting its
whole directory, and says in three places that it is a stopgap.

This replaces the stopgap with a declaration: a jail states the tools it needs, each declaration
states what becomes readable, executable and reachable on `PATH`, and a tool backed by a
`tddy-build` target is built on demand before the jail starts.

## Background

On 2026-09-26 a subagent in a live session lost its only content-search tool to
`Grep: spawn failed: No such file or directory`, fell back to globbing a path prefix that does not
exist in that repo, asked the same empty question thirty times, and then summarised confidently.
The per-call failure is why it went unnoticed: one refused tool among many, with no startup signal.

The narrower fact is that the bug was **recipe-specific**. `claude_interactive_policy()` already
grants `/opt/homebrew` and `/opt/local`; the `Shell` recipe the workspace jail uses does not. A
tool's availability depends on which recipe happened to be chosen, which is not a property anyone
declared or reviewed.

Most of the machinery this needs already exists. `tddy-build` is a content-addressed build system
with a DAG, an action cache and per-ecosystem plugins, and 45 live `BUILD.yaml` files.
`tddy-actions` already declares jail semantics. What is missing is a projection carrying
**read + exec + PATH** across the boundary, and any way for a session jail to ask for one.

## Proposed Changes

### What's Changing

**A tool declaration format at `<tddyhome>/tools/*.yaml`**, mirroring `<tddyhome>/agents/*.yaml`.
An entry names either a **build target id** or a **host binary**, and states what the jail gets:

```yaml
name: ripgrep
host_binary: rg                 # resolved on the host at plan time
sha256: "…"                     # required for a host binary; mismatch refuses the jail
bin_dirs: [.]                   # joined onto the jail's PATH
```

```yaml
name: tddy-tools
build_target: "tddy-tools:bin"  # built on demand; outputs come from the target
```

A declaration may `require:` others, resolved transitively.

**The sandbox invokes `tddy-build` when a build-target tool needs rebuilding.** Resolution and the
build happen **on the host before spawn** — the jail has no network
(`loopback_allow_ports: vec![]`, no egress shim), so nothing can install inside it.

**An unsatisfiable tool refuses the jail, naming the tool.** Not a per-call `spawn failed`.

**The grant is the binary, not its directory**, where the platform can express it.

**A tool set is resolved in `JailedWorkspaceSandboxProvisioner::provision`**, which all four jail
entry points funnel through — start, resume, jailed-codebase reprovision, and mid-session rebuild —
so a resumed session gets the same tools as a started one.

### What's Staying the Same

- **The jail still has no network.** Nothing installs inside it; every tool tree is materialised
  host-side.
- **One mount, the session's checkout.** A tool grant adds reads and exec, never a writable mount.
- **`tddy-build`'s existing `type: tool` is untouched.** It means something different — a `bin_dir`
  prepended to a dependent *build action's* `PATH`, with no output and no action — and keeps
  meaning it. The two must not be conflated; see *Naming*.
- **Recipes keep their baseline `exec_paths`.** Folding those into declarations is out of scope.

### Naming

`tddy-build` already has `type: tool`. This feature's concept is different in kind: a tree mounted
into a jail with read/exec/PATH grants, versus a directory added to a build action's `PATH`. The
new type must not be called `tool`. The PRD uses **`JailTool`** provisionally; the changeset
settles it.

## Impact Analysis

### Technical Impact

**The headline benefit needs a renderer change.** `packages/tddy-sandbox-darwin/src/profile.rs:155-162`
emits `process-exec*` only for reads where `exec && kind == ReadKind::Subpath`, so
`ReadSpec::literal(bin).executable()` is **silently dropped** on macOS. Per-binary exec grants need
a `Literal` arm. Linux already honours `exec` on every read kind and **ignores `policy.exec_paths`
entirely** — the two platforms disagree about what an exec grant is, and this change has to state
which one is the contract.

**A second silent-drop trap.** `SandboxBuilder::build()` (`builder.rs:458-473`) removes a read
shadowed by an enclosing `Subpath` — including an `exec: true` child under a non-exec parent. A
per-binary grant inside an already-granted tree would vanish.

**Four sharp edges in the build API**: `execute_target` returns `Ok` on a non-zero exit; there is
no "is it up to date?" API (`fingerprint_inputs` is private); `.tddy-build/out/` staging is
unimplemented so outputs are wherever the target declared them; target ids are opaque strings with
no label type.

**New state on the provisioner.** `JailedWorkspaceSandboxProvisioner` is a unit struct; reading
`<tddyhome>/tools/` needs a field on it or on `WorkspaceSandboxSpec`.

**Concurrency.** Two sessions needing one tool must not build it twice — the `JailRelaunch` keyed
gate is the pattern.

### User Impact

- **`Grep` works inside a sandboxed session**, for the first time on macOS.
- **A missing tool is a named refusal at session start**, not a silent per-call failure.
- **Session start can block on a build.** The content-addressed cache makes the warm path free; a
  cold build now sits between "start" and "session open", and a broken `BUILD.yaml` becomes a
  session that will not start. That is the deliberate trade against a silent failure.
- Operators can add a tool without a code change.

## Implementation Plan

1. **A jail-tool declaration type and loader** for `<tddyhome>/tools/*.yaml`, modelled on
   `load_agent_defs` — but **refusing duplicate names**, which that loader does not detect.
2. **Resolution**: transitive `require:`, cycle detection, one entry per name.
3. **Satisfaction**: a host binary is resolved and checksum-verified; a build target is located via
   `graph.actions_for(id)` outputs and built with `execute_target`, **checking `exit_code`**.
4. **Projection** into `MountSpec` / `ReadSpec::…executable()` / `exec_paths` / `PATH`, surviving
   the shadowing rule in `build()`.
5. **Renderer**: a `Literal` arm for exec in the Darwin profile, so a per-binary grant is real.
6. **Wiring** at `JailedWorkspaceSandboxProvisioner::provision`, with a keyed build gate.
7. **`ripgrep` as the first declaration**, and delete `FIXME(grep-in-jail)`.

## Acceptance Criteria

- [ ] AC1 — A jail declaring `ripgrep` can run `rg`; `Grep` returns matches inside a sandboxed
      session ([remote-codebase-mode.md](../remote-codebase-mode.md))
- [ ] AC2 — A jail declaring no tools has exactly today's executable surface
- [ ] AC3 — A declared tool that cannot be satisfied refuses the jail, naming the tool, at start
- [ ] AC4 — A host binary whose sha256 does not match refuses the jail, naming the mismatch
- [ ] AC5 — A host binary with no `sha256` is refused at load, naming the field
- [ ] AC6 — A build-target tool that is stale is rebuilt before the jail starts
- [ ] AC7 — A build-target tool that is current is **not** rebuilt
- [ ] AC8 — A build whose action exits non-zero refuses the jail — never `Ok` from `execute_target`
- [ ] AC9 — A build target with no declared `outputs` is refused at resolution, naming the target
- [ ] AC10 — Two sessions needing one uncached tool build it **once**
- [ ] AC11 — Transitive `require:` is resolved; a node tool declaring `node` gets both
- [ ] AC12 — A cycle in `require:` is refused, naming the cycle
- [ ] AC13 — Two declarations with the same `name` are refused, naming both files — unlike
      `load_agent_defs`, which silently lets `read_dir` order decide
- [ ] AC14 — An unknown key in a declaration is refused, naming the key (`deny_unknown_fields`)
- [ ] AC15 — A per-binary exec grant is rendered on macOS: `ReadSpec::literal(bin).executable()`
      produces a `process-exec*` entry ([sandboxed-codebase-mode.md](../../coder/sandboxed-codebase-mode.md))
- [ ] AC16 — A per-binary exec grant inside an enclosing read is **not** dropped by `build()`
- [ ] AC17 — A tool grant never makes a path writable
- [ ] AC18 — A resumed session has the same tool set as a started one (all four provision paths)
- [ ] AC19 — `FIXME(grep-in-jail)` and `host_ripgrep_dir` are gone
- [ ] AC20 — Documentation updated: the jail's tool surface, the declaration format, and the
      macOS/Linux difference in what an exec grant means

## References

### Affected Features

- [remote-codebase-mode.md](../remote-codebase-mode.md) — the workspace tool sandbox
- [sandboxed-codebase-mode.md](../../coder/sandboxed-codebase-mode.md) — jail confinement
- [tddy-build.md](../../build/tddy-build.md) — build targets as tool sources

### Related

- [Initial discovery](../../../dev/1-WIP/2026-09-26-sandbox-tool-specs-initial-discovery.md)
- [`plans/sandbox-toolspec-brief.md`](../../../../plans/sandbox-toolspec-brief.md) — the pre-planning brief
- PR #547 — the stopgap this replaces
