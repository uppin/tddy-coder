# tddy-build: content-addressed build system

**Product area:** Build
**Updated:** 2026-11-02
**Status:** Implemented — executor delegates to `tddy-actions`; tools are tool definitions; actions can run hermetically

## Unified action execution (2026-06-29)

Single-action execution in `executor.rs::run_action` delegates to `tddy-actions::ProcessRuntime`.
Each `BuildAction` is converted to `ActionSpec` via `action_convert.rs` and spawns a task in a
per-build `TaskRegistry` (observable stdout/stderr, cancellable). DAG wave scheduling, content-addressed
cache, and plugin lowering are unchanged. **Note:** `tddy-build` now depends on `tddy-actions` /
`tddy-task` (standalone-by-default property relaxed per approved plan).

## Summary

`tddy-build` is a standalone, Bazel-inspired build system for repository artifacts. It is a **generic build engine plus a wiring point for build plugins** — it has no knowledge of any language/ecosystem build target. Build targets are declared in `BUILD.yaml` files; the engine resolves each target's `type` to a registered `BuildPlugin` (or to one of its built-in structural types) which lowers the target into build actions. Targets lower to a global DAG of build actions, executed wave-by-wave with a content-addressed (SHA-256) action cache. The system is driven by `tddy-tools build` / `tddy-tools build-list` and is discoverable by the coding agent the same way session actions are — via the CLI.

It complements, but is distinct from, the two existing "action" concepts: `tddy-core` **session actions** (ephemeral agent capabilities) and the workflow **action cache** (per-session LLM fingerprint cache). Neither is a build graph.

## Plugin architecture

The engine knows nothing about specific build targets. A `BuildPlugin` declares the
`type` tag(s) it handles and lowers a target's config into `BuildAction`s:

```rust
pub trait BuildPlugin: Send + Sync {
    fn type_names(&self) -> &'static [&'static str]; // e.g. ["rust_binary", "rust_library"]
    fn lower(&self, ctx: &LowerContext) -> Result<Vec<BuildAction>, BuildError>;
}
```

Plugins are collected into a `PluginRegistry`, which the binaries assemble and pass
into the engine API (`service::build_json` / `build_list_json`). The
language/ecosystem recipes live in their **own crates**:

| Crate | Type(s) | Lowers to |
|-------|---------|-----------|
| `tddy-build-rust` | `rust_binary` | `cargo build -p <pkg> --bin <name> [--features …] [--release] [--target <triple>]` |
| `tddy-build-rust` | `rust_library` | `cargo build -p <pkg> [--features …] [--release]` |
| `tddy-build-typescript` | `typescript` | `bun run <build_script>` in `package_dir` |
| `tddy-build-docker` | `docker_image` | `docker build -f <dockerfile> -t <tag> [--build-arg …] <context>` |
| `tddy-build-buildroot` | `buildroot_image` | `make O=<out> <defconfig>` then `make O=<out>` in `buildroot_dir` |
| `tddy-build-qemu` | `qemu_disk_image` | `qemu-img convert -f <fmt> -O qcow2 <input> <output>` |

### Built-in structural types

The engine keeps three types built in, because they are part of the build-graph
vocabulary itself rather than ecosystem recipes:

| Type | Behavior |
|------|----------|
| `script` | the declared `command` argv (generic escape hatch) |
| `tool` | no build action — a tool definition (inline or `ref`) whose binary is on the `PATH` of every action naming it in `tool_dep_ids` — see [Tool targets](#tool-targets) |
| `group` | no own action — its members become build-order predecessors |

A target whose `type` is neither a built-in nor a registered plugin fails with
`unknown target type: <name>`. Hermetic tests assert command construction and
`--dry-run` output; real subprocess execution is covered for `script`/`tool`, with
`cargo`/`bun`/`docker` execution behind environment availability gates.

## Authoring format

```yaml
schema_version: 1
targets:
  - id: "packages/app:bin"
    name: "App binary"
    deps: ["packages/core:lib"]
    config:
      type: rust_binary
      package: app
      bin_name: app
      profile: release
```

The `config` map is open: `type` selects the plugin/built-in, and the remaining keys
are that handler's config (parsed by the plugin). Explicit `actions` and enum-valued
action fields (`ActionType`, `OutputKind`, authored as snake_case strings) are
unchanged. `BuildAction` and the cache types remain proto messages — the stable
engine↔plugin contract.

### Tool targets

A `type: tool` target **is a declared tool** — the same `ToolDef` coding-agent jails use
([Sandbox builder § Declared tools](../coder/sandbox-builder.md#declared-tools)). Its config is one of:

- **an inline definition** — `schema_version`, `id`, `summary`, `binary`, and optionally `deps`,
  `reads`, `network`. Its `id` joins the run's tool registry; an id the registry already holds is
  refused as a duplicate;
- **`ref: <tool id>`** — names a tool in the registry: the built-ins plus `<tddyhome>/tools`
  overrides. A `ref` carrying definition fields is refused.

A tool's binary may be **another target's output**, so a tool can be built and then used:

```yaml
targets:
  - id: "tools:stamp"
    config:
      type: tool
      schema_version: 1
      id: stamp
      summary: stamps its input
      binary: { command: stamp, target: { target: "stamp:bin", path: target/debug/stamp } }
  - id: "app:build"
    actions:
      - id: package
        type: command
        command: ["sh", "-c", "stamp < lib.txt > app.txt"]
        tool_dep_ids: ["tools:stamp"]
        sandbox: true
```

- `tool_dep_ids` name **tool targets**; each resolves through the registry with its dependency
  closure (deps before dependents), and the directory of every resolved binary is put on the
  action's `PATH`.
- A target named by a tool's `binary.target` becomes a build-order predecessor of every action that
  lists the tool, and a cycle through a tool target is reported naming it. That edge is added for
  actions declared in `BUILD.yaml`; plugin-lowered actions do not get it — see
  [plugin-lowered tool deps get no build order](../../dev/todo/2026-11-02-plugin-lowered-tool-deps-get-no-build-order.md).
- The legacy `{bin_dir, commands}` shape is refused, with a message naming the target and the
  migration to a tool definition.

`packages/tddy-build/examples/pipeline/app/BUILD.yaml` is the worked example: `tools:stamp-build`
installs `tools/out/stamp`, and the inline `tools:bin` tool's binary is that output.

### `buildroot_image` config

```yaml
config:
  type: buildroot_image
  defconfig: qemu_x86_64_defconfig     # required — make target to configure
  buildroot_dir: "external/buildroot"  # required — repo-root-relative path to Buildroot tree
  output_dir: "build/br-out"           # required — repo-root-relative Buildroot output dir (O=)
  srcs: ["board/my-os/"]               # optional — cache invalidation globs
  outputs:                             # optional — default: <output_dir>/images/rootfs.ext4
    - path: "build/br-out/images/rootfs.ext4"
      kind: file
```

Emits two `BuildAction`s. The first runs `make O=<rel> <defconfig>` and declares `<output_dir>/.config` as its output. The second runs `make O=<rel>` and declares `.config` as its input — this wires sequencing via the engine's action-overlap edge inference. `O=<rel>` is computed as the relative path from `buildroot_dir` to `output_dir` so `make` writes to the correct location when running inside `buildroot_dir`. `jobs`/parallelism is not a config field; set `MAKEFLAGS=-j$(nproc)` in the environment.

### `qemu_disk_image` config

```yaml
config:
  type: qemu_disk_image
  input: "build/br-out/images/rootfs.ext4"  # required — repo-root-relative
  input_format: raw                          # optional, default "raw"
  srcs: ["build/br-out/images/rootfs.ext4"] # optional — cache invalidation globs
  outputs:                                   # optional — default: swap input ext to .qcow2
    - path: "build/br-out/images/rootfs.qcow2"
      kind: file
```

Emits one `BuildAction`: `qemu-img convert -f <input_format> -O qcow2 <input> <output>`. Runs from the repo root (all paths repo-root-relative).

### End-to-end: OS image → QEMU qcow2

```yaml
schema_version: 1
targets:
  - id: "my-os:rootfs"
    name: "Buildroot rootfs"
    config:
      type: buildroot_image
      defconfig: qemu_x86_64_defconfig
      buildroot_dir: "external/buildroot"
      output_dir: "build/br-out"
      srcs: ["board/my-os/"]

  - id: "my-os:qcow2"
    name: "QEMU disk image"
    deps: ["my-os:rootfs"]
    config:
      type: qemu_disk_image
      input: "build/br-out/images/rootfs.ext4"
```

`my-os:rootfs` runs Buildroot and caches the ext4 image independently. If only the `qemu_disk_image` config changes, Buildroot does not re-run. `make` and `qemu-img` are both provided by the Nix dev shell (`pkgs.gnumake`, `pkgs.qemu`).

## Hermetic actions

An action with **`sandbox: true`** — or every action of a **`tddy-tools build --hermetic`** run —
runs in a jail where:

- the concrete paths its `inputs` match are read-only;
- its declared outputs are the only writable paths;
- its tool closure is the only set of executables beyond the jail's baseline;
- its network is the lowering of the closure's `network:` sections, so a closure with no
  network-declaring tool has **no network**.

An undeclared write or an undeclared exec fails the action, and its error names the path. The
engine spawns a hermetic action through the **`ActionSandbox` port** it is handed; without one, a
hermetic action is refused, never run unconfined. The production implementation is
`tddy-daemon-sandbox`'s `DaemonActionSandbox` (Seatbelt on macOS, cgroups on Linux), and a local
`tddy-tools build` jails actions under `<tddyhome>/build-jails`. `--hermetic` is not relayed: with
`TDDY_SOCKET` set it is an error, since the session's build would run unconfined.

Hermetic is opt-in per action or per run. Known limits:

- **Linux** confines network and resources but not writes or execs: the cgroups jail shares the host
  filesystem root — [hermetic build actions unconfined on Linux](../../dev/todo/2026-11-02-hermetic-build-actions-unconfined-on-linux.md).
- An action can execute the outputs it writes — [outputs are executable](../../dev/todo/2026-11-02-hermetic-build-action-outputs-are-executable.md).
- A closure whose tools declare egress hosts is refused: build actions have no egress shim —
  [no egress shim](../../dev/todo/2026-11-02-hermetic-build-actions-have-no-egress-shim.md).
- A failing action in a dependency target does not stop the build (all actions, hermetic or not) —
  [dependency failure does not stop the build](../../dev/todo/2026-11-02-build-dependency-failure-does-not-stop-the-build.md).

## Action cache

- Location: `{repo_root}/.tddy-build/cache/{target_id}/{action_id}.json`.
- Key: `sha256:<hex>` over the action id, type, command, env, input file fingerprints (`path:size:mtime_ms`), declared outputs, tool deps, the resolved tool binaries' fingerprints, and whether the action runs hermetically — order-independent. Changing a tool's binary invalidates every action using it, and a hermetic run never reuses an unconfined run's entry.
- Hit requires: stored key matches the recomputed key **and** every declared output still exists on disk.
- Writes are atomic (tmp + `sync_all` + rename), mirroring `tddy-core`'s action-cache flush.
- Modes: `readwrite` (default), `readonly`/`offline` (read local, never write); `--no-cache` bypasses both.

## CLI & relay

- `tddy-tools build-list --repo-dir <dir> [--query …] [--limit …] [--offset …]` → `{"targets":[…],"total":N}`.
- `tddy-tools build --repo-dir <dir> --target <id> [--no-cache] [--dry-run] [--hermetic]` → build record JSON. A local build resolves tools against the built-ins plus `<tddyhome>/tools` (`TDDY_DATA_DIR`, else the profile default, else `~/.tddy`).
- Both are relay-capable via `TDDY_SOCKET`: when set, the request is forwarded to the host session, where `tddy-coder` has registered a `tddy_core::BuildExecutor`. `tddy-core` defines the extension trait and wire types only — it has **no** dependency on `tddy-build`; the dependency and wiring live in `tddy-coder`.

## Crate boundaries

```
tddy-build  (engine + plugin wiring point — depends on tddy-actions, tddy-task; no sandbox crate)
   ▲ implemented-against by the plugin crates:
   │     tddy-build-rust, tddy-build-typescript, tddy-build-docker
   │     tddy-build-buildroot, tddy-build-qemu
   ▲ assembled into a PluginRegistry by:
   │     tddy-tools  (local exec / relay client)
   │     tddy-coder  (executor impl; relay host)
tddy-core   (BuildExecutor trait + wire types + ToolCallResponse variants; no tddy-build dep)
```

Hermetic actions run through `tddy-build`'s `ActionSandbox` port; `tddy-daemon-sandbox` implements it and the binaries wire it. `tddy-build` depends on none of the plugin crates; the plugin crates depend only on
`tddy-build`. The binaries (`tddy-tools`, `tddy-coder`) are the only places that know
the concrete plugin set — they register `RustPlugin`/`TypeScriptPlugin`/`DockerPlugin`
into a `PluginRegistry` and pass it into the engine.

## Out of scope (v1)

Distributed/parent-fallback cache; full remote build execution; watch mode; final output-publication convention (`.tddy-build/out/{target_id}/` staging only); cross-compilation architecture filtering. See [docs/dev/todo/2026-06-16-tddy-build.md](../../dev/todo/2026-06-16-tddy-build.md).

## Related

- `packages/tddy-build/docs/architecture.md` — crate architecture; `docs/dev/changesets/` — cross-package changeset history
- `packages/tddy-build/` — implementation
- Prior art: `~/Code/makers-lt/maker-build` (two-phase TypeScript build system)
