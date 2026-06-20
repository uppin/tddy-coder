# tddy-build: Buildroot + QEMU disk image plugins

**Date**: 2026-06-20
**Status**: Approved

## Summary

Two new `tddy-build` plugin crates — `tddy-build-buildroot` and `tddy-build-qemu` — that together let a BUILD.yaml recipe produce a bootable QEMU qcow2 disk image from a Buildroot defconfig.

## Scope

- `buildroot_image` plugin: runs `make <defconfig>` + `make` against a Buildroot source tree
- `qemu_disk_image` plugin: converts the raw/ext4 Buildroot output to a standalone qcow2 via `qemu-img convert`
- C++: out of scope (Buildroot's internal cross-compiler handles all C++ for the OS image)

## Package structure

Two new crates, following the exact pattern of `tddy-build-rust`, `tddy-build-docker`, `tddy-build-typescript`:

```
packages/tddy-build-buildroot/
  Cargo.toml
  src/lib.rs                        ← BuildrootPlugin
  examples/
    os-image/
      BUILD.yaml
  tests/
    example_os_image.rs

packages/tddy-build-qemu/
  Cargo.toml
  src/lib.rs                        ← QemuPlugin
  examples/
    qemu-image/
      BUILD.yaml
  tests/
    example_qemu_image.rs
```

Each crate depends only on `tddy-build`. No cross-dependency between them.

## `buildroot_image` plugin

**Type name:** `buildroot_image`

### Config schema

```yaml
config:
  type: buildroot_image
  defconfig: qemu_x86_64_defconfig     # required — make target to configure
  buildroot_dir: "external/buildroot"  # required — relative to repo root
  output_dir: "build/br-out"           # required — Buildroot O= output directory
  srcs: ["board/my-os/"]               # optional — cache invalidation globs
  outputs:                             # optional — inferred if absent
    - path: "build/br-out/images/rootfs.ext4"
      kind: file
```

All paths are repo-root-relative. No absolute paths in BUILD.yaml.

**`buildroot_dir`** maps to `BuildAction.working_dir`. The executor resolves `repo_root.join(buildroot_dir)` to an absolute path before spawning the process, so `make` runs from the correct directory without a `-C` flag.

**`output_dir`** is passed as `O=<output_dir>` in the make argv.

**`outputs` inference:** if `outputs` is empty, the plugin defaults to `[{ path: "<output_dir>/images/rootfs.ext4", kind: file }]`. Users override when the defconfig produces a different image name or format.

**`jobs`:** not a plugin field. Parallelism is the build system's concern — users set `MAKEFLAGS=-j$(nproc)` in the Nix dev shell environment.

### Actions emitted (two `BuildAction`s)

```
Action 1 — id: "buildroot-defconfig"
  command:     ["make", "O=<output_dir>", "<defconfig>"]
  working_dir: <buildroot_dir>
  outputs:     [{ path: "<output_dir>/.config", kind: file }]

Action 2 — id: "buildroot-build"
  command:     ["make", "O=<output_dir>"]
  working_dir: <buildroot_dir>
  inputs:      [FileSet { include: ["<output_dir>/.config"] }]
  outputs:     [{ path: "<output_dir>/images/rootfs.ext4", kind: file }]  ← or explicit override
```

The executor's `action_overlap_edges` creates an edge from Action 1 → Action 2 because Action 1 declares `<output_dir>/.config` as an output and Action 2 declares it as an input. Without this explicit declaration, both actions would land in wave 0 and run in parallel. The intermediate `.config` is always declared by the plugin regardless of whether the user specifies `outputs` — only the final rootfs output is inferred/overridden.

### Error conditions

- `defconfig` empty → `BuildError::Manifest`
- `buildroot_dir` empty → `BuildError::Manifest`
- `output_dir` empty → `BuildError::Manifest`
- unknown field in config → `BuildError::Manifest` (via `deny_unknown_fields`)

## `qemu_disk_image` plugin

**Type name:** `qemu_disk_image`

### Config schema

```yaml
config:
  type: qemu_disk_image
  input: "build/br-out/images/rootfs.ext4"   # required — repo-root-relative
  input_format: raw                            # optional, default "raw"
  srcs: ["build/br-out/images/rootfs.ext4"]   # optional — cache invalidation globs
  outputs:                                     # optional — inferred if absent
    - path: "build/br-out/images/rootfs.qcow2"
      kind: file
```

**`outputs` inference:** if `outputs` is empty, the plugin derives the output path from `input` by replacing the file extension with `.qcow2`.

### Action emitted (one `BuildAction`)

```
Action — id: "qemu-disk-image"
  command: ["qemu-img", "convert", "-f", <input_format>, "-O", "qcow2", <input>, <outputs[0].path>]
  working_dir: ""   (runs from repo root; all paths are repo-root-relative)
```

### Error conditions

- `input` empty → `BuildError::Manifest`
- `outputs` empty and `input` has no extension to replace → `BuildError::Manifest`
- unknown field in config → `BuildError::Manifest`

## End-to-end BUILD.yaml example

Minimal — no explicit `outputs` needed:

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

`my-os:rootfs` runs Buildroot and caches the ext4 image independently of the conversion step. If only the `qemu_disk_image` config changes, Buildroot does not re-run.

## Tool availability

Both `make` (for the fake-buildroot fixture) and `qemu-img` are provided by the Nix dev shell. No skip guards in tests.

## Testing

### Unit tests (`src/lib.rs` in each crate)

**`tddy-build-buildroot`:**
- `defconfig_action_has_correct_argv` — `["make", "O=<dir>", "<defconfig>"]`, `working_dir` set
- `build_action_has_correct_argv` — `["make", "O=<dir>"]`, `working_dir` set
- `inferred_output_defaults_to_rootfs_ext4`
- `explicit_outputs_override_default`
- `missing_defconfig_is_rejected`
- `missing_buildroot_dir_is_rejected`
- `missing_output_dir_is_rejected`
- `unknown_field_is_rejected`

**`tddy-build-qemu`:**
- `convert_action_has_correct_argv` — `["qemu-img", "convert", "-f", "raw", "-O", "qcow2", ...]`
- `inferred_output_swaps_extension_to_qcow2`
- `explicit_outputs_override_default`
- `custom_input_format_is_used` — `-f qcow2` when `input_format: qcow2`
- `missing_input_is_rejected`
- `unknown_field_is_rejected`

### Integration tests (no skip guards)

**`tests/example_os_image.rs`** — uses a fixture fake-buildroot at `examples/fake-buildroot/` with a minimal `Makefile` that mimics Buildroot's interface using real `make`:

```makefile
.DEFAULT_GOAL := build

%:
	mkdir -p $(O)
	touch $(O)/.config

build:
	mkdir -p $(O)/images
	dd if=/dev/zero of=$(O)/images/rootfs.ext4 bs=1M count=1
```

`%:` is a GNU Make match-anything pattern rule that accepts any defconfig target (e.g. `make O=… qemu_x86_64_defconfig`). The explicit `build:` rule takes precedence for the default goal.

Tests:
- `buildroot_defconfig_action_creates_config_file`
- `buildroot_build_action_creates_rootfs_image`
- `buildroot_cache_hits_on_rerun`
- `buildroot_cache_miss_after_config_change`

**`tests/example_qemu_image.rs`** — uses real `qemu-img` from Nix; creates a 1 MiB raw file with `dd`, converts it:

Tests:
- `qemu_disk_image_converts_raw_to_qcow2`
- `qemu_disk_image_cache_hits_on_rerun`
- `qemu_disk_image_cache_miss_after_input_change`

## Future

Once tddy-build implements the `.tddy-build/out/{target_id}/` staging convention (noted as TODO in the architecture doc), the plugin-level output inference can be replaced by engine-provided output paths, and `outputs` can be removed from both plugin configs entirely.
