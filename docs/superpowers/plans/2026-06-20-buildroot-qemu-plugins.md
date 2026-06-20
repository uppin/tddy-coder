# Buildroot + QEMU Disk Image Plugins Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add two `tddy-build` plugin crates — `tddy-build-buildroot` and `tddy-build-qemu` — so a BUILD.yaml recipe can run Buildroot to produce an OS image and convert it to a bootable QEMU qcow2 disk image.

**Architecture:** `tddy-build-buildroot` implements the `buildroot_image` plugin type, emitting two `BuildAction`s (`make <defconfig>` then `make`) with an explicit `.config` intermediate that wires their sequencing. `tddy-build-qemu` implements `qemu_disk_image`, emitting a single `qemu-img convert` action. Both follow the existing plugin pattern (thin crate, `BuildPlugin` trait, `serde_yaml` config parsing, unit tests in `src/lib.rs`, integration tests in `tests/`).

**Tech Stack:** Rust, `serde`/`serde_yaml`, `tddy-build` plugin trait, `tempfile` (integration tests), `tokio` (async executor in integration tests), GNU `make`, `qemu-img`.

## Global Constraints

- All paths in BUILD.yaml and plugin configs are repo-root-relative — no absolute paths.
- `outputs` field is optional in both plugin configs; the plugin infers a default if omitted.
- `jobs` / parallelism is NOT a plugin config field — users set `MAKEFLAGS` in the environment.
- No skip guards in integration tests — `make` and `qemu-img` are always available via the Nix dev shell.
- Follow `deny_unknown_fields` on all config structs.
- New crates must be added to the root `Cargo.toml` workspace members list.
- Run all tests with `./test` (writes output to `.verify-result.txt`); read that file to confirm results.

---

## File Map

**Created:**
- `packages/tddy-build-buildroot/Cargo.toml`
- `packages/tddy-build-buildroot/src/lib.rs` — `BuildrootPlugin` + unit tests
- `packages/tddy-build-buildroot/examples/fake-buildroot/Makefile` — fixture for integration tests
- `packages/tddy-build-buildroot/examples/os-image/BUILD.yaml` — example usage
- `packages/tddy-build-buildroot/tests/example_os_image.rs` — integration tests
- `packages/tddy-build-qemu/Cargo.toml`
- `packages/tddy-build-qemu/src/lib.rs` — `QemuPlugin` + unit tests
- `packages/tddy-build-qemu/examples/qemu-image/BUILD.yaml` — example usage
- `packages/tddy-build-qemu/tests/example_qemu_image.rs` — integration tests

**Modified:**
- `Cargo.toml` (root) — add both crates to `workspace.members`

---

## Task 1: `tddy-build-buildroot` — plugin + unit tests

**Files:**
- Create: `packages/tddy-build-buildroot/Cargo.toml`
- Create: `packages/tddy-build-buildroot/src/lib.rs`
- Create: `packages/tddy-build-buildroot/examples/os-image/BUILD.yaml`
- Modify: `Cargo.toml` (root workspace)

**Interfaces:**
- Produces: `pub struct BuildrootPlugin` — implements `tddy_build::plugin::BuildPlugin`, registered under type name `"buildroot_image"`.

- [ ] **Step 1: Add `tddy-build-buildroot` to the workspace**

In the root `Cargo.toml`, find the `members = [` list and add `"packages/tddy-build-buildroot"` after `"packages/tddy-build-docker"`.

- [ ] **Step 2: Create `packages/tddy-build-buildroot/Cargo.toml`**

```toml
[package]
name = "tddy-build-buildroot"
version = "0.1.0"
edition = "2021"
description = "Buildroot plugin for tddy-build: lowers buildroot_image targets to `make`."

[lib]
name = "tddy_build_buildroot"
path = "src/lib.rs"

[dependencies]
tddy-build = { path = "../tddy-build" }
serde = { version = "1", features = ["derive"] }
serde_yaml = "0.9"

[dev-dependencies]
tempfile = "3"
tokio = { version = "1", features = ["rt", "rt-multi-thread", "macros"] }
```

- [ ] **Step 3: Write the failing unit tests in `packages/tddy-build-buildroot/src/lib.rs`**

```rust
use serde::Deserialize;

use tddy_build::plugin::{BuildPlugin, LowerContext};
use tddy_build::proto::{ActionType, BuildAction, FileSet, OutputDecl, OutputKind};
use tddy_build::BuildError;

pub struct BuildrootPlugin;

impl BuildPlugin for BuildrootPlugin {
    fn type_names(&self) -> &'static [&'static str] {
        &["buildroot_image"]
    }

    fn lower(&self, _ctx: &LowerContext) -> Result<Vec<BuildAction>, BuildError> {
        unimplemented!()
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct BuildrootImage {
    defconfig: String,
    buildroot_dir: String,
    output_dir: String,
    srcs: Vec<String>,
    outputs: Vec<tddy_build::OutputSpec>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lower(fields_yaml: &str) -> Vec<BuildAction> {
        let config: serde_yaml::Value = serde_yaml::from_str(fields_yaml).expect("valid yaml");
        let ctx = LowerContext {
            type_name: "buildroot_image",
            target_id: "t",
            target_name: "",
            deps: &[],
            config: &config,
        };
        BuildrootPlugin.lower(&ctx).expect("lower")
    }

    fn lower_err(fields_yaml: &str) -> BuildError {
        let config: serde_yaml::Value = serde_yaml::from_str(fields_yaml).expect("valid yaml");
        let ctx = LowerContext {
            type_name: "buildroot_image",
            target_id: "t",
            target_name: "",
            deps: &[],
            config: &config,
        };
        BuildrootPlugin.lower(&ctx).expect_err("expected error")
    }

    #[test]
    fn defconfig_action_has_correct_argv() {
        let actions = lower("defconfig: qemu_x86_64_defconfig\nbuildroot_dir: external/buildroot\noutput_dir: build/br-out\n");
        assert_eq!(
            actions[0].command,
            vec!["make", "O=build/br-out", "qemu_x86_64_defconfig"]
        );
        assert_eq!(actions[0].id, "buildroot-defconfig");
        assert_eq!(actions[0].working_dir, "external/buildroot");
    }

    #[test]
    fn build_action_has_correct_argv() {
        let actions = lower("defconfig: qemu_x86_64_defconfig\nbuildroot_dir: external/buildroot\noutput_dir: build/br-out\n");
        assert_eq!(actions[1].command, vec!["make", "O=build/br-out"]);
        assert_eq!(actions[1].id, "buildroot-build");
        assert_eq!(actions[1].working_dir, "external/buildroot");
    }

    #[test]
    fn intermediate_config_wires_defconfig_to_build() {
        let actions = lower("defconfig: qemu_x86_64_defconfig\nbuildroot_dir: external/buildroot\noutput_dir: build/br-out\n");
        assert_eq!(actions[0].outputs[0].path, "build/br-out/.config");
        assert_eq!(actions[1].inputs[0].include, vec!["build/br-out/.config"]);
    }

    #[test]
    fn inferred_output_defaults_to_rootfs_ext4() {
        let actions = lower("defconfig: qemu_x86_64_defconfig\nbuildroot_dir: external/buildroot\noutput_dir: build/br-out\n");
        assert_eq!(actions[1].outputs[0].path, "build/br-out/images/rootfs.ext4");
    }

    #[test]
    fn explicit_outputs_override_default() {
        let actions = lower(
            "defconfig: qemu_x86_64_defconfig\nbuildroot_dir: external/buildroot\noutput_dir: build/br-out\noutputs:\n  - path: build/br-out/images/rootfs.img\n    kind: file\n",
        );
        assert_eq!(actions[1].outputs[0].path, "build/br-out/images/rootfs.img");
    }

    #[test]
    fn missing_defconfig_is_rejected() {
        assert!(matches!(
            lower_err("buildroot_dir: external/buildroot\noutput_dir: build/br-out\n"),
            BuildError::Manifest(_)
        ));
    }

    #[test]
    fn missing_buildroot_dir_is_rejected() {
        assert!(matches!(
            lower_err("defconfig: qemu_x86_64_defconfig\noutput_dir: build/br-out\n"),
            BuildError::Manifest(_)
        ));
    }

    #[test]
    fn missing_output_dir_is_rejected() {
        assert!(matches!(
            lower_err("defconfig: qemu_x86_64_defconfig\nbuildroot_dir: external/buildroot\n"),
            BuildError::Manifest(_)
        ));
    }

    #[test]
    fn unknown_field_is_rejected() {
        assert!(matches!(
            lower_err("defconfig: x\nbuildroot_dir: d\noutput_dir: o\nbogus: 1\n"),
            BuildError::Manifest(_)
        ));
    }
}
```

- [ ] **Step 4: Run tests — confirm they fail with `unimplemented!()`**

```bash
./test -p tddy-build-buildroot
```

Read `.verify-result.txt`. Expected: tests fail with `not yet implemented`.

- [ ] **Step 5: Implement `BuildrootPlugin::lower`**

Replace the `unimplemented!()` impl in `src/lib.rs` with the full implementation:

```rust
impl BuildPlugin for BuildrootPlugin {
    fn type_names(&self) -> &'static [&'static str] {
        &["buildroot_image"]
    }

    fn lower(&self, ctx: &LowerContext) -> Result<Vec<BuildAction>, BuildError> {
        let cfg: BuildrootImage = serde_yaml::from_value(ctx.config.clone())
            .map_err(|e| BuildError::Manifest(format!("invalid buildroot_image config: {e}")))?;

        if cfg.defconfig.is_empty() {
            return Err(BuildError::Manifest(
                "buildroot_image: defconfig is required".into(),
            ));
        }
        if cfg.buildroot_dir.is_empty() {
            return Err(BuildError::Manifest(
                "buildroot_image: buildroot_dir is required".into(),
            ));
        }
        if cfg.output_dir.is_empty() {
            return Err(BuildError::Manifest(
                "buildroot_image: output_dir is required".into(),
            ));
        }

        let o_arg = format!("O={}", cfg.output_dir);
        let config_path = format!("{}/.config", cfg.output_dir);

        let final_outputs = if cfg.outputs.is_empty() {
            vec![tddy_build::OutputSpec {
                path: format!("{}/images/rootfs.ext4", cfg.output_dir),
                kind: "file".to_string(),
            }]
        } else {
            cfg.outputs.clone()
        };

        let defconfig_action = BuildAction {
            id: "buildroot-defconfig".to_string(),
            description: format!("make {}", cfg.defconfig),
            r#type: ActionType::Command as i32,
            command: vec!["make".to_string(), o_arg.clone(), cfg.defconfig.clone()],
            inputs: tddy_build::srcs_to_inputs(&cfg.srcs, ""),
            outputs: vec![OutputDecl {
                path: config_path.clone(),
                kind: OutputKind::File as i32,
            }],
            working_dir: cfg.buildroot_dir.clone(),
            ..Default::default()
        };

        let build_action = BuildAction {
            id: "buildroot-build".to_string(),
            description: "make".to_string(),
            r#type: ActionType::Command as i32,
            command: vec!["make".to_string(), o_arg],
            inputs: vec![FileSet {
                include: vec![config_path],
                exclude: Vec::new(),
                root: String::new(),
            }],
            outputs: tddy_build::outputs_to_decls(&final_outputs)?,
            working_dir: cfg.buildroot_dir,
            ..Default::default()
        };

        Ok(vec![defconfig_action, build_action])
    }
}
```

- [ ] **Step 6: Run tests — confirm all pass**

```bash
./test -p tddy-build-buildroot
```

Read `.verify-result.txt`. Expected: all unit tests pass.

- [ ] **Step 7: Create `packages/tddy-build-buildroot/examples/os-image/BUILD.yaml`**

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
```

- [ ] **Step 8: Commit**

```bash
git add packages/tddy-build-buildroot/ Cargo.toml
git commit -m "feat(tddy-build-buildroot): buildroot_image plugin with unit tests"
```

---

## Task 2: `tddy-build-buildroot` — integration tests

**Files:**
- Create: `packages/tddy-build-buildroot/examples/fake-buildroot/Makefile`
- Create: `packages/tddy-build-buildroot/tests/example_os_image.rs`

**Interfaces:**
- Consumes: `pub struct BuildrootPlugin` from Task 1.

- [ ] **Step 1: Create the fake-buildroot fixture**

Create `packages/tddy-build-buildroot/examples/fake-buildroot/Makefile`:

```makefile
.DEFAULT_GOAL := build

%:
	mkdir -p $(O)
	touch $(O)/.config

build:
	mkdir -p $(O)/images
	dd if=/dev/zero of=$(O)/images/rootfs.ext4 bs=1M count=1
```

`%:` is a GNU Make match-anything pattern rule — accepts any defconfig target (e.g. `make O=… qemu_x86_64_defconfig`). The explicit `build:` rule takes precedence for the default goal.

- [ ] **Step 2: Write `packages/tddy-build-buildroot/tests/example_os_image.rs`**

```rust
use std::path::PathBuf;
use std::sync::Arc;

use tddy_build::executor::{execute_target, ExecuteOptions};
use tddy_build::graph::BuildGraph;
use tddy_build::plugin::PluginRegistry;
use tddy_build_buildroot::BuildrootPlugin;

fn fake_buildroot_src() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/fake-buildroot")
}

fn registry() -> PluginRegistry {
    let mut r = PluginRegistry::new();
    r.register(Arc::new(BuildrootPlugin));
    r
}

fn staged() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let br_dir = dir.path().join("external/buildroot");
    std::fs::create_dir_all(&br_dir).expect("mkdir");
    std::fs::copy(fake_buildroot_src().join("Makefile"), br_dir.join("Makefile"))
        .expect("copy Makefile");
    dir
}

fn load_graph(output_dir: &str) -> BuildGraph {
    let yaml = format!(
        "schema_version: 1\ntargets:\n  - id: \"my-os:rootfs\"\n    config:\n      type: buildroot_image\n      defconfig: qemu_x86_64_defconfig\n      buildroot_dir: external/buildroot\n      output_dir: {output_dir}\n"
    );
    let manifest = tddy_build::load_build_manifest(&yaml).expect("parse");
    BuildGraph::from_manifests(vec![manifest]).expect("graph")
}

#[tokio::test]
async fn buildroot_defconfig_action_creates_config_file() {
    let dir = staged();
    let graph = load_graph("build/br-out");
    let record = execute_target(
        dir.path(),
        &graph,
        "my-os:rootfs",
        &ExecuteOptions::default(),
        &registry(),
    )
    .await
    .expect("execute");
    assert_eq!(
        record.actions[0].exit_code,
        0,
        "defconfig stderr: {}",
        record.actions[0].stderr
    );
    assert!(
        dir.path().join("build/br-out/.config").exists(),
        ".config must be created by defconfig action"
    );
}

#[tokio::test]
async fn buildroot_build_action_creates_rootfs_image() {
    let dir = staged();
    let graph = load_graph("build/br-out");
    let record = execute_target(
        dir.path(),
        &graph,
        "my-os:rootfs",
        &ExecuteOptions::default(),
        &registry(),
    )
    .await
    .expect("execute");
    assert_eq!(
        record.actions[1].exit_code,
        0,
        "build stderr: {}",
        record.actions[1].stderr
    );
    assert!(
        dir.path().join("build/br-out/images/rootfs.ext4").exists(),
        "rootfs.ext4 must be produced"
    );
}

#[tokio::test]
async fn buildroot_cache_hits_on_rerun() {
    let dir = staged();
    let opts = ExecuteOptions::default();
    let reg = registry();
    let graph = load_graph("build/br-out");

    execute_target(dir.path(), &graph, "my-os:rootfs", &opts, &reg)
        .await
        .expect("first run");

    let second = execute_target(dir.path(), &graph, "my-os:rootfs", &opts, &reg)
        .await
        .expect("second run");
    assert!(second.actions[1].cached, "rerun must be a cache hit");
}

#[tokio::test]
async fn buildroot_cache_miss_after_makefile_edit() {
    let dir = staged();
    let opts = ExecuteOptions::default();
    let reg = registry();

    // Use srcs to track the Makefile
    let yaml = "schema_version: 1\ntargets:\n  - id: \"my-os:rootfs\"\n    config:\n      type: buildroot_image\n      defconfig: qemu_x86_64_defconfig\n      buildroot_dir: external/buildroot\n      output_dir: build/br-out\n      srcs: [\"external/buildroot/Makefile\"]\n";
    let manifest = tddy_build::load_build_manifest(yaml).expect("parse");
    let graph = BuildGraph::from_manifests(vec![manifest]).expect("graph");

    execute_target(dir.path(), &graph, "my-os:rootfs", &opts, &reg)
        .await
        .expect("first run");

    // Touch the tracked Makefile to change its mtime
    let makefile = dir.path().join("external/buildroot/Makefile");
    let contents = std::fs::read(&makefile).expect("read");
    std::fs::write(&makefile, contents).expect("rewrite");

    let third = execute_target(dir.path(), &graph, "my-os:rootfs", &opts, &reg)
        .await
        .expect("third run");
    assert!(
        !third.actions[1].cached,
        "Makefile edit must invalidate the cache"
    );
}
```

- [ ] **Step 3: Run integration tests — confirm all pass**

```bash
./test -p tddy-build-buildroot
```

Read `.verify-result.txt`. Expected: all 4 integration tests and all unit tests pass.

- [ ] **Step 4: Commit**

```bash
git add packages/tddy-build-buildroot/examples/fake-buildroot/ packages/tddy-build-buildroot/tests/
git commit -m "test(tddy-build-buildroot): fake-buildroot fixture + integration tests"
```

---

## Task 3: `tddy-build-qemu` — plugin + unit tests

**Files:**
- Create: `packages/tddy-build-qemu/Cargo.toml`
- Create: `packages/tddy-build-qemu/src/lib.rs`
- Create: `packages/tddy-build-qemu/examples/qemu-image/BUILD.yaml`
- Modify: `Cargo.toml` (root workspace)

**Interfaces:**
- Produces: `pub struct QemuPlugin` — implements `tddy_build::plugin::BuildPlugin`, registered under type name `"qemu_disk_image"`.

- [ ] **Step 1: Add `tddy-build-qemu` to the workspace**

In the root `Cargo.toml`, add `"packages/tddy-build-qemu"` to the `members` list after `"packages/tddy-build-buildroot"`.

- [ ] **Step 2: Create `packages/tddy-build-qemu/Cargo.toml`**

```toml
[package]
name = "tddy-build-qemu"
version = "0.1.0"
edition = "2021"
description = "QEMU disk image plugin for tddy-build: lowers qemu_disk_image targets to `qemu-img convert`."

[lib]
name = "tddy_build_qemu"
path = "src/lib.rs"

[dependencies]
tddy-build = { path = "../tddy-build" }
serde = { version = "1", features = ["derive"] }
serde_yaml = "0.9"

[dev-dependencies]
tempfile = "3"
tokio = { version = "1", features = ["rt", "rt-multi-thread", "macros"] }
```

- [ ] **Step 3: Write the failing unit tests in `packages/tddy-build-qemu/src/lib.rs`**

```rust
use serde::Deserialize;

use tddy_build::plugin::{BuildPlugin, LowerContext};
use tddy_build::proto::{ActionType, BuildAction};
use tddy_build::BuildError;

pub struct QemuPlugin;

impl BuildPlugin for QemuPlugin {
    fn type_names(&self) -> &'static [&'static str] {
        &["qemu_disk_image"]
    }

    fn lower(&self, _ctx: &LowerContext) -> Result<Vec<BuildAction>, BuildError> {
        unimplemented!()
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct QemuDiskImage {
    input: String,
    input_format: String,
    srcs: Vec<String>,
    outputs: Vec<tddy_build::OutputSpec>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lower(fields_yaml: &str) -> BuildAction {
        let config: serde_yaml::Value = serde_yaml::from_str(fields_yaml).expect("valid yaml");
        let ctx = LowerContext {
            type_name: "qemu_disk_image",
            target_id: "t",
            target_name: "",
            deps: &[],
            config: &config,
        };
        let mut actions = QemuPlugin.lower(&ctx).expect("lower");
        assert_eq!(actions.len(), 1);
        actions.remove(0)
    }

    fn lower_err(fields_yaml: &str) -> BuildError {
        let config: serde_yaml::Value = serde_yaml::from_str(fields_yaml).expect("valid yaml");
        let ctx = LowerContext {
            type_name: "qemu_disk_image",
            target_id: "t",
            target_name: "",
            deps: &[],
            config: &config,
        };
        QemuPlugin.lower(&ctx).expect_err("expected error")
    }

    #[test]
    fn convert_action_has_correct_argv() {
        let action = lower("input: build/br-out/images/rootfs.ext4\n");
        assert_eq!(
            action.command,
            vec![
                "qemu-img",
                "convert",
                "-f",
                "raw",
                "-O",
                "qcow2",
                "build/br-out/images/rootfs.ext4",
                "build/br-out/images/rootfs.qcow2",
            ]
        );
        assert_eq!(action.id, "qemu-disk-image");
    }

    #[test]
    fn inferred_output_swaps_extension_to_qcow2() {
        let action = lower("input: build/br-out/images/rootfs.ext4\n");
        assert_eq!(action.outputs[0].path, "build/br-out/images/rootfs.qcow2");
    }

    #[test]
    fn explicit_outputs_override_default() {
        let action = lower(
            "input: build/br-out/images/rootfs.ext4\noutputs:\n  - path: build/my-os.qcow2\n    kind: file\n",
        );
        assert_eq!(action.outputs[0].path, "build/my-os.qcow2");
        assert_eq!(action.command.last().unwrap(), "build/my-os.qcow2");
    }

    #[test]
    fn custom_input_format_is_used() {
        let action = lower("input: build/rootfs.qcow2\ninput_format: qcow2\n");
        assert_eq!(action.command[3], "qcow2");
    }

    #[test]
    fn default_input_format_is_raw() {
        let action = lower("input: build/rootfs.ext4\n");
        assert_eq!(action.command[3], "raw");
    }

    #[test]
    fn missing_input_is_rejected() {
        assert!(matches!(lower_err("\n"), BuildError::Manifest(_)));
    }

    #[test]
    fn unknown_field_is_rejected() {
        assert!(matches!(
            lower_err("input: x.ext4\nbogus: 1\n"),
            BuildError::Manifest(_)
        ));
    }
}
```

- [ ] **Step 4: Run tests — confirm they fail with `unimplemented!()`**

```bash
./test -p tddy-build-qemu
```

Read `.verify-result.txt`. Expected: tests fail with `not yet implemented`.

- [ ] **Step 5: Implement `QemuPlugin::lower`**

Replace the `unimplemented!()` impl in `src/lib.rs`:

```rust
impl BuildPlugin for QemuPlugin {
    fn type_names(&self) -> &'static [&'static str] {
        &["qemu_disk_image"]
    }

    fn lower(&self, ctx: &LowerContext) -> Result<Vec<BuildAction>, BuildError> {
        let cfg: QemuDiskImage = serde_yaml::from_value(ctx.config.clone())
            .map_err(|e| BuildError::Manifest(format!("invalid qemu_disk_image config: {e}")))?;

        if cfg.input.is_empty() {
            return Err(BuildError::Manifest(
                "qemu_disk_image: input is required".into(),
            ));
        }

        let input_format = if cfg.input_format.is_empty() {
            "raw".to_string()
        } else {
            cfg.input_format.clone()
        };

        let output_path = if cfg.outputs.is_empty() {
            infer_qcow2_path(&cfg.input)?
        } else {
            cfg.outputs[0].path.clone()
        };

        let final_outputs = if cfg.outputs.is_empty() {
            vec![tddy_build::OutputSpec {
                path: output_path.clone(),
                kind: "file".to_string(),
            }]
        } else {
            cfg.outputs.clone()
        };

        Ok(vec![BuildAction {
            id: "qemu-disk-image".to_string(),
            description: format!("qemu-img convert {}", cfg.input),
            r#type: ActionType::Command as i32,
            command: vec![
                "qemu-img".to_string(),
                "convert".to_string(),
                "-f".to_string(),
                input_format,
                "-O".to_string(),
                "qcow2".to_string(),
                cfg.input.clone(),
                output_path,
            ],
            inputs: tddy_build::srcs_to_inputs(&cfg.srcs, ""),
            outputs: tddy_build::outputs_to_decls(&final_outputs)?,
            ..Default::default()
        }])
    }
}

fn infer_qcow2_path(input: &str) -> Result<String, BuildError> {
    let p = std::path::Path::new(input);
    let stem = p
        .file_stem()
        .ok_or_else(|| {
            BuildError::Manifest(format!(
                "qemu_disk_image: cannot infer output path from input {input:?}"
            ))
        })?
        .to_string_lossy();
    match p.parent() {
        Some(parent) if parent != std::path::Path::new("") => {
            Ok(format!("{}/{}.qcow2", parent.to_string_lossy(), stem))
        }
        _ => Ok(format!("{stem}.qcow2")),
    }
}
```

- [ ] **Step 6: Run tests — confirm all pass**

```bash
./test -p tddy-build-qemu
```

Read `.verify-result.txt`. Expected: all unit tests pass.

- [ ] **Step 7: Create `packages/tddy-build-qemu/examples/qemu-image/BUILD.yaml`**

```yaml
schema_version: 1
targets:
  - id: "my-os:qcow2"
    name: "QEMU disk image"
    config:
      type: qemu_disk_image
      input: "build/br-out/images/rootfs.ext4"
```

- [ ] **Step 8: Commit**

```bash
git add packages/tddy-build-qemu/ Cargo.toml
git commit -m "feat(tddy-build-qemu): qemu_disk_image plugin with unit tests"
```

---

## Task 4: `tddy-build-qemu` — integration tests

**Files:**
- Create: `packages/tddy-build-qemu/tests/example_qemu_image.rs`

**Interfaces:**
- Consumes: `pub struct QemuPlugin` from Task 3.

- [ ] **Step 1: Write `packages/tddy-build-qemu/tests/example_qemu_image.rs`**

```rust
use std::sync::Arc;

use tddy_build::executor::{execute_target, ExecuteOptions};
use tddy_build::graph::BuildGraph;
use tddy_build::plugin::PluginRegistry;
use tddy_build_qemu::QemuPlugin;

fn registry() -> PluginRegistry {
    let mut r = PluginRegistry::new();
    r.register(Arc::new(QemuPlugin));
    r
}

fn staged() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let images = dir.path().join("build/br-out/images");
    std::fs::create_dir_all(&images).expect("mkdir");
    // 1 MiB zero-filled raw "disk image" — valid input for qemu-img convert -f raw
    std::fs::write(images.join("rootfs.ext4"), vec![0u8; 1024 * 1024])
        .expect("write raw image");
    dir
}

fn load_graph() -> BuildGraph {
    let yaml = "schema_version: 1\ntargets:\n  - id: \"my-os:qcow2\"\n    config:\n      type: qemu_disk_image\n      input: build/br-out/images/rootfs.ext4\n      srcs: [\"build/br-out/images/rootfs.ext4\"]\n";
    let manifest = tddy_build::load_build_manifest(yaml).expect("parse");
    BuildGraph::from_manifests(vec![manifest]).expect("graph")
}

#[tokio::test]
async fn qemu_disk_image_converts_raw_to_qcow2() {
    let dir = staged();
    let record = execute_target(
        dir.path(),
        &load_graph(),
        "my-os:qcow2",
        &ExecuteOptions::default(),
        &registry(),
    )
    .await
    .expect("execute");
    assert_eq!(
        record.actions[0].exit_code,
        0,
        "stderr: {}",
        record.actions[0].stderr
    );
    let qcow2_path = dir.path().join("build/br-out/images/rootfs.qcow2");
    assert!(qcow2_path.exists(), "qcow2 output must be created");

    // Verify qcow2 magic header: first 4 bytes are b"QFI\xfb"
    let header = std::fs::read(&qcow2_path).expect("read qcow2");
    assert_eq!(&header[..4], b"QFI\xfb", "output must be a valid qcow2 image");
}

#[tokio::test]
async fn qemu_disk_image_cache_hits_on_rerun() {
    let dir = staged();
    let opts = ExecuteOptions::default();
    let reg = registry();
    let graph = load_graph();

    execute_target(dir.path(), &graph, "my-os:qcow2", &opts, &reg)
        .await
        .expect("first run");

    let second = execute_target(dir.path(), &graph, "my-os:qcow2", &opts, &reg)
        .await
        .expect("second run");
    assert!(second.actions[0].cached, "rerun must be a cache hit");
}

#[tokio::test]
async fn qemu_disk_image_cache_miss_after_input_change() {
    let dir = staged();
    let opts = ExecuteOptions::default();
    let reg = registry();
    let graph = load_graph();

    execute_target(dir.path(), &graph, "my-os:qcow2", &opts, &reg)
        .await
        .expect("first run");

    // Overwrite the raw image with different content to change its fingerprint
    let raw_path = dir.path().join("build/br-out/images/rootfs.ext4");
    std::fs::write(&raw_path, vec![1u8; 1024 * 1024]).expect("modify raw image");

    let third = execute_target(dir.path(), &graph, "my-os:qcow2", &opts, &reg)
        .await
        .expect("third run");
    assert!(
        !third.actions[0].cached,
        "modified input must invalidate the cache"
    );
}
```

- [ ] **Step 2: Run integration tests — confirm all pass**

```bash
./test -p tddy-build-qemu
```

Read `.verify-result.txt`. Expected: all 3 integration tests and all unit tests pass.

- [ ] **Step 3: Run the full test suite to confirm no regressions**

```bash
./test
```

Read `.verify-result.txt`. Expected: all tests pass across all packages.

- [ ] **Step 4: Commit**

```bash
git add packages/tddy-build-qemu/tests/
git commit -m "test(tddy-build-qemu): real qemu-img integration tests with cache verification"
```
