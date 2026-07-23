# BSP build server (BSP-shaped build-target RPC service)

> **Related:** consumes the [session catalog](session-catalog.md) as its data source and the
> [multi-transport RPC](rpc-multi-transport.md) stack as its wire. Build targets are discovered from
> `BUILD.yaml` exactly as today; this feature adds a richer projection of them and a service to serve it.

## Status (2026-07-22)

**Planned.** Producer (catalog) exists; this feature enriches what is persisted per build target and
adds the `bsp.BspService` RPC.

## Purpose

Expose the repository's build targets through a **Build Server Protocol (BSP)–shaped** RPC service
served by `tddy-coder`, so the web UI (and future tooling) can:

- **enumerate** every build target with its capabilities, tags, languages, dependencies, source roots,
  and output paths; and
- **drive** the target's build lifecycle: compile, test, run.

The shape (methods + data model) mirrors the real [Build Server Protocol](https://build-server-protocol.github.io),
but rides the workspace's existing protobuf/Connect + LiveKit RPC transports rather than JSON-RPC 2.0.
It is therefore **not** wire-compatible with external BSP clients (Metals, IntelliJ-BSP) — that is a
[non-goal](#non-goals).

## BSP → tddy mapping

| BSP concept | tddy realization |
|---|---|
| `workspace/buildTargets` | `WorkspaceBuildTargets` — every `BUILD.yaml` target as a `BuildTarget` |
| `BuildTarget.id` / `displayName` | target `id` (e.g. `packages/foo:binary`) / `name` |
| `BuildTarget.baseDirectory` | directory of the target's `BUILD.yaml`, relative to repo root |
| `BuildTarget.tags` | `application` / `library` / `test`, author-declared or derived from `config.type` |
| `BuildTarget.languageIds` | `rust` / `typescript` / …, author-declared or derived from `config.type` |
| `BuildTarget.dependencies` | the target's `deps` (declare-time edges) |
| `BuildTargetCapabilities` | `{ canCompile, canTest, canRun, canDebug }` |
| `buildTarget/sources` | `BuildTargetSources` — union of the target's lowered action input globs |
| `buildTarget/outputPaths` | `BuildTargetOutputPaths` — the target's declared `OutputDecl` paths |
| `workspace/reload` | `WorkspaceReload` — re-run the catalog populate task |
| `buildTarget/compile` \| `test` \| `run` | `BuildTargetCompile` \| `BuildTargetTest` \| `BuildTargetRun` |

## Data model

### Manifest (`BUILD.yaml`) — new optional fields

`BuildTarget` gains three optional, back-compatible fields (existing manifests parse unchanged):

```yaml
targets:
  - id: packages/foo:lib
    name: Foo library
    tags: [library]              # optional; else derived from config.type
    languages: [rust]            # optional; else derived from config.type
    capabilities:                # optional; else derived from config.type
      compile: true
      test: true
      run: false
      debug: false
    config: { type: rust_library, package: foo }
```

### Derivation (when a field is omitted)

Resolved from `config.type`:

| type | languages | tags | capabilities |
|---|---|---|---|
| `rust_binary` | `rust` | `application` | compile, test, run |
| `rust_library` | `rust` | `library` | compile, test |
| `typescript` | `typescript` | — | compile, test, run |
| `docker_image` / `buildroot_image` / `qemu_disk_image` | — | `application` | compile |
| `script` | — | — | compile |
| `tool` / `group` | — | (structural) | — |

Author-declared values always win over derivation.

### Persistence — dedicated `build_targets` table

The per-session catalog (`<session_dir>/catalog.db`) gains a dedicated, typed `build_targets` table —
the **authoritative** source the BSP reads — written in the same populate transaction as the existing
unified `catalog` table:

| column | contents |
|---|---|
| `id` (PK) | target id |
| `name`, `package`, `base_dir`, `target_type` | display name, projected package, base directory, `config.type` |
| `tags`, `languages`, `deps`, `sources`, `outputs` | JSON arrays |
| `can_compile`, `can_test`, `can_run`, `can_debug` | resolved capability flags |
| `source_path` | absolute `BUILD.yaml` path |

`sources`/`outputs` are derived by **lowering** each target to its build actions and collecting the
action input globs / declared output paths — no per-plugin special-casing.

The existing lightweight `build_target` rows in the shared `catalog` table are unchanged, so the
current unified `list`/`list_for_package` behavior is preserved.

## Behaviour

- **Read methods** (`WorkspaceBuildTargets`, `BuildTargetSources`, `BuildTargetOutputPaths`) read the
  `build_targets` table via `SessionCatalog`, blocking on the first populate exactly like today's
  catalog reads.
- **`WorkspaceReload`** re-spawns the populate task; the next read reflects `BUILD.yaml` edits,
  additions, and deletions.
- **Build ops** (`BuildTargetCompile` / `Test` / `Run`) execute the target in the corresponding
  `BuildMode` via `tddy_build::service::build_json`, returning per-action outcomes and a status. A mode
  the target's resolved capabilities do not permit is **rejected** (no silent fallback).
- Served over both the Connect `/rpc` endpoint and the LiveKit participant surface (one impl instance),
  and discoverable via gRPC server reflection.

## Acceptance criteria

1. `WorkspaceBuildTargets` returns every `BUILD.yaml` target with its id, display name, base directory,
   tags, language ids, dependencies, and capability flags.
2. Capabilities/tags/languages are author-declared when present and derived from `config.type`
   otherwise; a declared value overrides derivation.
3. `BuildTargetSources` returns a target's source globs; `BuildTargetOutputPaths` returns its declared
   outputs.
4. `BuildTargetCompile` compiles a target; `BuildTargetTest`/`BuildTargetRun` execute the test/run
   lifecycle for targets that declare (or derive) the capability.
5. Invoking `BuildTargetTest`/`BuildTargetRun` on a target lacking the capability is rejected with a
   clear error and runs nothing.
6. `WorkspaceReload` followed by a read reflects a changed `BUILD.yaml` (added/removed/edited targets).
7. The service is reachable over `/rpc` and LiveKit and appears in server reflection as `bsp.BspService`.

## Non-goals

- Literal JSON-RPC 2.0 BSP transport / external BSP-client compatibility (Metals, IntelliJ-BSP).
- Structured diagnostics (file/line/severity) — build ops return exit code + raw stdout/stderr as today.
- `buildTarget/inverseSources`, `dependencySources`, `dependencyModules`, `resources`,
  `debugSession/start`.
- Streaming compile/test progress (BSP task notifications) — the initial methods are unary.
