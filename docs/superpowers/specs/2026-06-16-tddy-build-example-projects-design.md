# tddy-build example projects, logging, and verification — design

**Date:** 2026-06-16
**Status:** Approved (design); pending implementation plan
**Scope packages:** `tddy-build`, `tddy-build-rust`, `tddy-build-typescript`, `tddy-build-docker`

## Problem

`tddy-build` is a Bazel-inspired, content-addressed build engine plus a plugin
wiring point. It ships an engine (`tddy-build`) with built-in structural target
types (`script`/`tool`/`group`) and three recipe plugin crates
(`tddy-build-rust` → `rust_binary`/`rust_library`, `tddy-build-typescript` →
`typescript`, `tddy-build-docker` → `docker_image`).

There are **no example projects** exercising the build packages end to end, and
two contract gaps block trustworthy examples:

1. **No logging.** The engine depends on `log` but never calls it — there is zero
   instrumentation in discovery, lowering, cycle detection, the cache, or the
   executor. The user has made adequate logging a prerequisite.
2. **Plugin targets cannot cache correctly.** Plugin-lowered actions carry empty
   `inputs`/`outputs`, so a rust/ts/docker target's cache never invalidates on a
   source edit (the TS plugin even parses `output_dirs` but leaves it unused).

## Goals

- Add example **multi-package, multi-target** projects with **interdependent
  targets**, one per build package, that build with **real toolchains via the nix
  dev shell** (`cargo`, `bun` from nix; `docker` system binary, daemon required).
- Make the action cache behave correctly for plugin targets (hit on rebuild,
  **miss on source edit**).
- Provide tests that verify: adequate logging, circular-reference detection,
  target-references-target dependencies, real builds succeed, and the action
  cache works as expected.

## Non-goals (YAGNI / documented v1 non-goals)

Distributed/parent-fallback cache, hermetic sandboxing, remote build execution,
watch mode, output-publication conventions, cross-compilation filtering. No
changes unrelated to wiring inputs/outputs.

## Environment (verified)

- nix dev shell provides `cargo`/`rustc` and `bun`/`node` (see `flake.nix`
  `packages`). Docker is **not** in the shell; `/usr/bin/docker` exists and the
  daemon is currently reachable.
- Tests run inside the nix shell (`./test`, `./dev cargo test`), so spawned build
  actions inherit `cargo`/`bun` on `PATH`.

## Deliverables

### 1. Logging instrumentation (prerequisite — implement and verify first)

Add `log::{debug,info,warn}` at the meaningful engine seams (the workspace logging
facility is the `log` crate, as used by `tddy-core`):

- `discovery` — number of manifests found and their paths (debug).
- `lower` — each target lowered: `type` → N actions (debug).
- `graph::from_manifests` — target count (debug); **`warn!` naming the cycle when
  detection fires**.
- `cache` — key computed (trace/debug), hit / miss / persist (debug).
- `executor` — target start, wave scheduling, per-action command + exit code +
  `cached` flag (debug/info).

**Verification:** `packages/tddy-build/tests/logging.rs`, in its own integration
test binary, installs a tiny in-process `log::Log` capturer (records into a
`Mutex<Vec<String>>` — **no new dependency**), runs the engine pipeline example,
and asserts the key events appear (discovery, lowering, cache miss→hit, action
execution) and that a cyclic manifest emits the cycle `warn!`. Written red-first:
it fails today (zero logging) and passes after instrumentation. Kept isolated in
its own file because `log` has a single process-global logger.

### 2. Plugin extension — emit inputs/outputs

Add a shared helper in `tddy-build` (e.g. `plugin::declared_io(fields) ->
Result<(Vec<FileSet>, Vec<OutputDecl>), BuildError>`) that parses optional `srcs`
(input include globs, with optional `root`) and `outputs` (`{path, kind}`) from a
plugin's open config and maps them onto the proto `FileSet`/`OutputDecl` types.
Each plugin calls it and attaches the result to its lowered `BuildAction`:

- **rust** (`rust_binary`/`rust_library`): accept `srcs` + `outputs`; set
  `working_dir` to the package/workspace dir. New keys added to the
  `deny_unknown_fields` structs.
- **typescript** (`typescript`): accept `srcs`; wire the existing `output_dirs`
  into outputs as `directory` kind (remove the `#[allow(dead_code)]`).
- **docker** (`docker_image`): accept `srcs` (context + Dockerfile) as inputs;
  when an output path is declared, add `--iidfile <path>` to the `docker build`
  argv and declare that path as the action's file output, giving docker a real
  file to fingerprint.

This makes the content-addressed cache meaningful for plugin targets: identical
inputs → hit; edited declared source → key changes → miss.

### 3. Example projects (committed on disk, one per package)

Discovered via the engine glob `**/{BUILD,build}.{yaml,yml}` so multi-package
discovery is exercised.

| Package | Project path | Interdependent targets | Execution |
|---|---|---|---|
| `tddy-build` (engine) | `packages/tddy-build/examples/pipeline/` | `script`/`tool`/`group`: codegen → lib → app, plus a `tool` target providing a binary on dependents' `PATH` | real (built-ins, no toolchain) |
| `tddy-build-rust` | `packages/tddy-build-rust/examples/workspace/` | `rust_library` core ← `rust_library` util ← `rust_binary` app | real `cargo build` |
| `tddy-build-typescript` | `packages/tddy-build-typescript/examples/monorepo/` | `typescript` shared ← ui ← web | real `bun build` |
| `tddy-build-docker` | `packages/tddy-build-docker/examples/images/` | `docker_image` base ← api, base ← worker | real `docker build` |

Each project is a tree of sub-package `BUILD.yaml` files whose targets declare
`deps` on targets in sibling packages (cross-package interdependency).

**Isolation from outer workspaces:**

- The rust example declares its **own `[workspace]`** in its `Cargo.toml`, and the
  root `Cargo.toml` adds an `exclude` entry for the example path, so the example
  crates are not members of the tddy-coder workspace (avoids cargo's
  "not a member" error and accidental workspace builds).
- The typescript example is self-contained: its own `package.json`(s) with a
  build script that needs no install (`bun build ./src/... --outdir dist`). Root
  `package.json` workspaces are an explicit list that does not match the example
  path.

### 4. Verification tests

One integration test file per crate (plugin examples live in their own crate
because `tddy-build` must not depend on the plugin crates; the engine example
lives in `tddy-build`). Each loads its on-disk project via
`discover_build_manifests` and `BuildGraph::from_manifests`.

- **Target-as-dependency:** `build_order` / `waves` resolve dependencies
  deps-first across packages (e.g. app after util after core).
- **Builds succeed (real):** `execute_target` runs the real toolchain, asserts
  exit code 0 and that declared artifacts exist. `cargo`/`bun`/`docker` execution
  tests **skip-with-log when the tool is unavailable** (a test-level guard via
  `command -v` / `docker info`, not a production code branch), keeping the suite
  green on machines without that toolchain.
- **Action cache (real):** first run executes; second run is a cache hit (the
  toolchain is skipped); editing a declared source input forces a miss and a
  rebuild — meaningful for every package because of deliverable 2.
- **Circular reference detection:** assert `BuildGraph::from_manifests` errors for
  a 3-node cycle and a self-loop at the engine level, plus one **plugin-typed**
  cycle (two `rust_*` targets depending on each other) to confirm detection is
  type-agnostic.

## Approach

Test-driven throughout: the logging capture test and the example verification
tests are written first (red), then instrumentation, plugin changes, and example
fixtures make them green. Logging (deliverable 1) lands first as the prerequisite
gate, then plugin inputs/outputs (2), then the example projects and tests (3, 4).

## Risks

- Recursive `cargo`/`bun`/`docker` invoked from within `cargo test`: kept tiny;
  example crates/packages are minimal.
- Docker requires a running daemon — guarded by a skip-with-log check so absence
  does not fail the suite.
- Example crates/packages must stay isolated from the outer Cargo/bun workspaces
  (handled via own `[workspace]` + root `exclude`, and self-contained
  `package.json`).
