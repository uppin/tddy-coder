# tddy-build Example Projects, Logging & Verification — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add real-toolchain, multi-package, interdependent example build projects for each tddy-build package, after instrumenting the engine with logging and wiring inputs/outputs into the recipe plugins so the action cache is correct.

**Architecture:** Three phases. (A) Add `log` instrumentation to the engine seams and prove it with a capture-logger test. (B) Add a shared inputs/outputs helper to `tddy-build` and have the rust/typescript/docker plugins emit `inputs`/`outputs` on their lowered actions. (C) Add one committed, real-building example project per package (`script`/`tool`/`group`, `rust_*`, `typescript`, `docker_image`) plus integration tests covering target-as-dependency, real build success, action-cache hit/miss-on-edit, and circular-reference detection.

**Tech Stack:** Rust (tddy-build engine + plugin crates), `log` crate, `serde_yaml`, prost protos (`BuildAction`/`FileSet`/`OutputDecl`), real `cargo`/`bun` (nix dev shell) and `docker` (system, daemon-gated). Run everything inside the nix shell via `./dev cargo test …`.

**Key facts baked into this plan (verified against the code):**
- All file paths in actions resolve **relative to `repo_root`**: input `FileSet.root` (executor.rs:242-246), `working_dir` (executor.rs:175-179), and output existence `repo_root.join(output)` (cache.rs:113).
- A target may carry explicit `actions` **and** a `config`; explicit actions run first, then the lowered config action (lower.rs).
- The cache is a hit only when the recorded key matches **and** every declared output path still exists (cache.rs:99-118). With no declared inputs/outputs, a plugin action always hits on rerun and never invalidates — this is the gap Phase B fixes.
- `tddy-build` must not depend on the plugin crates; engine-only tests use inline plugins/YAML (see `tests/build_acceptance.rs`).
- The `log` crate has a single process-global logger, so the capture test lives in its own integration-test binary.

---

## File Structure

**Phase A — logging (engine `packages/tddy-build/src/`):**
- Modify `discovery.rs`, `lower.rs`, `graph.rs`, `executor.rs`, `cache.rs` — add `log::{debug,info,warn,trace}` calls.
- Create `tests/logging.rs` — capture-logger verification (own binary).

**Phase B — plugin inputs/outputs:**
- Create `packages/tddy-build/src/io.rs` — `OutputSpec`, `srcs_to_inputs`, `outputs_to_decls`; re-export from `lib.rs`.
- Modify `packages/tddy-build-rust/src/lib.rs`, `tddy-build-typescript/src/lib.rs`, `tddy-build-docker/src/lib.rs` — parse `srcs`/`outputs` (ts: `srcs` + existing `output_dirs`), attach to the lowered `BuildAction`.

**Phase C — example projects + tests (one per package):**
- `packages/tddy-build/examples/pipeline/**` + `packages/tddy-build/tests/example_pipeline.rs`
- `packages/tddy-build-rust/examples/workspace/**` + `packages/tddy-build-rust/tests/example_workspace.rs`
- `packages/tddy-build-typescript/examples/monorepo/**` + `packages/tddy-build-typescript/tests/example_monorepo.rs`
- `packages/tddy-build-docker/examples/images/**` + `packages/tddy-build-docker/tests/example_images.rs`
- Modify root `Cargo.toml` — `exclude` the rust example workspace.
- Modify `packages/tddy-build/docs/architecture.md` via the changeset workflow (`docs/dev/1-WIP/`).

**Commands:** all test commands run inside the nix shell, e.g. `./dev cargo test -p tddy-build --test logging`. The `tddy-build-rust` and `tddy-build-typescript` plugin crates already depend on `tddy-build`; their integration tests also need `tempfile` as a dev-dependency (the engine already uses it). Add `tempfile = "3"` under `[dev-dependencies]` where noted.

---

## Task 0: Create the feature branch

**Files:** none (git only). The repo is currently on `master`.

- [ ] **Step 1: Branch**

```bash
cd /var/tddy/Code/tddy-coder
git checkout -b feat/tddy-build-examples
```

- [ ] **Step 2: Confirm baseline builds**

Run: `./dev cargo build -p tddy-build -p tddy-build-rust -p tddy-build-typescript -p tddy-build-docker`
Expected: compiles clean (no warnings about new code yet).

---

## Task 1: Logging capture test (red)

**Files:**
- Create: `packages/tddy-build/tests/logging.rs`

This single-test binary installs a process-global capturing logger, then drives the
engine (discovery → lower → execute twice → a cyclic graph) and asserts the expected
log lines appear. It fails now because the engine emits nothing.

- [ ] **Step 1: Write the failing test**

```rust
//! Verifies the engine emits adequate logging at its key seams. Runs in its own
//! test binary because `log` has a single process-global logger.

use std::sync::{Mutex, OnceLock};

use tddy_build::discovery::discover_build_manifests;
use tddy_build::executor::{execute_target, ExecuteOptions};
use tddy_build::graph::BuildGraph;
use tddy_build::plugin::PluginRegistry;

static LOG_BUFFER: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

fn buffer() -> &'static Mutex<Vec<String>> {
    LOG_BUFFER.get_or_init(|| Mutex::new(Vec::new()))
}

struct CaptureLogger;

impl log::Log for CaptureLogger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        buffer()
            .lock()
            .unwrap()
            .push(format!("{} {}", record.level(), record.args()));
    }
    fn flush(&self) {}
}

fn install_logger() {
    let _ = log::set_boxed_logger(Box::new(CaptureLogger));
    log::set_max_level(log::LevelFilter::Trace);
}

fn captured() -> Vec<String> {
    buffer().lock().unwrap().clone()
}

fn assert_logged(needle: &str) {
    let lines = captured();
    assert!(
        lines.iter().any(|l| l.contains(needle)),
        "expected a log line containing {needle:?}; captured:\n{}",
        lines.join("\n")
    );
}

const SCRIPT_YAML: &str = r#"
schema_version: 1
targets:
  - id: "log:demo"
    name: "Log Demo"
    actions:
      - id: "write"
        type: command
        command: ["sh", "-c", "echo run >> marker.txt"]
        inputs:
          - include: ["input.txt"]
            root: "."
        outputs:
          - path: "marker.txt"
            kind: file
"#;

const CYCLE_YAML: &str = r#"
schema_version: 1
targets:
  - id: "a:t"
    name: A
    deps: ["b:t"]
    config: { type: script, command: ["true"] }
  - id: "b:t"
    name: B
    deps: ["a:t"]
    config: { type: script, command: ["true"] }
"#;

#[tokio::test]
async fn engine_logs_discovery_lowering_cache_and_cycles() {
    install_logger();

    let repo = tempfile::tempdir().expect("tempdir");
    let root = repo.path();
    std::fs::write(root.join("BUILD.yaml"), SCRIPT_YAML).expect("write BUILD.yaml");
    std::fs::write(root.join("input.txt"), "seed").expect("seed input");

    // Discovery should log how many manifests it found.
    let discovered = discover_build_manifests(root).expect("discover");
    let manifests = discovered.into_iter().map(|(_, m)| m).collect::<Vec<_>>();
    assert_logged("manifest");

    let graph = BuildGraph::from_manifests(manifests).expect("graph");
    let opts = ExecuteOptions::default();
    let registry = PluginRegistry::new();

    // First run: lowering + cache miss + action execution.
    let first = execute_target(root, &graph, "log:demo", &opts, &registry)
        .await
        .expect("first run");
    assert!(!first.actions[0].cached);
    assert_logged("lowered target");
    assert_logged("building target");
    assert_logged("cache miss");

    // Second run: cache hit.
    let second = execute_target(root, &graph, "log:demo", &opts, &registry)
        .await
        .expect("second run");
    assert!(second.actions[0].cached);
    assert_logged("cache hit");

    // Cycle detection must warn and name the cycle.
    let cyclic = tddy_build::load_build_manifest(CYCLE_YAML).expect("parse cyclic");
    assert!(BuildGraph::from_manifests(vec![cyclic]).is_err());
    assert_logged("cycle");
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `./dev cargo test -p tddy-build --test logging`
Expected: FAIL — the assertions `assert_logged("manifest")` (and the rest) panic because the engine emits no log lines.

- [ ] **Step 3: Commit the red test**

```bash
git add packages/tddy-build/tests/logging.rs
git commit -m "test(tddy-build): add engine logging capture test (red)"
```

---

## Task 2: Instrument the engine with logging (green)

**Files:**
- Modify: `packages/tddy-build/src/discovery.rs`
- Modify: `packages/tddy-build/src/lower.rs`
- Modify: `packages/tddy-build/src/graph.rs`
- Modify: `packages/tddy-build/src/executor.rs`
- Modify: `packages/tddy-build/src/cache.rs`

Use fully-qualified `log::` macros (the crate already depends on `log`; no `use` needed). The log message substrings must match the test needles: `manifest`, `lowered target`, `building target`, `cache miss`, `cache hit`, `cycle`.

- [ ] **Step 1: Log discovery**

In `discovery.rs`, after `paths.sort(); paths.dedup();`, before reading manifests, add:

```rust
    log::debug!("discovered {} build manifest(s)", paths.len());
    for path in &paths {
        log::trace!("build manifest: {}", path.display());
    }
```

- [ ] **Step 2: Log lowering**

In `lower.rs` `lower_target`, replace the final `Ok(actions)` with:

```rust
    if let Some(config) = &target.config {
        log::debug!(
            "lowered target {} (type {}) into {} action(s)",
            target.id,
            config.r#type,
            actions.len()
        );
    } else {
        log::debug!(
            "lowered target {} into {} explicit action(s)",
            target.id,
            actions.len()
        );
    }
    Ok(actions)
```

- [ ] **Step 3: Log cycle detection and target count**

In `graph.rs`, cycle detection happens in `visit_build_order` (the `on_stack` check). Add a `warn!` immediately before that error return — replace:

```rust
        if on_stack.iter().any(|v| v == id) {
            return Err(BuildError::Cycle(format!("cycle through target {id}")));
        }
```

with:

```rust
        if on_stack.iter().any(|v| v == id) {
            log::warn!("build graph cycle detected through target {id}");
            return Err(BuildError::Cycle(format!("cycle through target {id}")));
        }
```

And in `from_manifests`, add a target-count debug line just before `let graph = Self { targets, order };`:

```rust
        log::debug!("build graph: {} target(s)", order.len());
```

- [ ] **Step 4: Log execution and cache decisions**

In `executor.rs`:

In `execute_target`, at the start of the `for current_target in &order` loop body (after fetching actions, before the `is_empty` continue), add:

```rust
        log::info!("building target {}", current_target);
```

In `run_one`, replace the cache-hit early return block and surrounding logic so the hit/miss/persist are logged. Specifically, after `let cache_key = compute_cache_key(action, &fingerprints);` and the hit check:

```rust
    if use_cache && lookup_cache(repo_root, target_id, &action.id, &cache_key).is_some() {
        log::debug!("cache hit for action {} (target {})", action.id, target_id);
        return Ok(ActionOutcome {
            action_id: action.id.clone(),
            cached: true,
            exit_code: 0,
            argv: action.command.clone(),
            stdout: String::new(),
            stderr: String::new(),
        });
    }
    log::debug!("cache miss for action {} (target {})", action.id, target_id);
    log::debug!("running action {}: {:?}", action.id, action.command);
```

And after a successful persist (inside the `if outcome.exit_code == 0 && use_cache && opts.cache_mode.writes()` block, after `persist_cache(...)?;`):

```rust
        log::debug!("persisted cache entry for action {} (target {})", action.id, target_id);
```

- [ ] **Step 5: Log the computed cache key (trace)**

In `cache.rs` `compute_cache_key`, just before returning the final key string, add:

```rust
    log::trace!("cache key for action {}: {}", action.id, key);
```

(use the actual variable name holding the `sha256:`-prefixed result).

- [ ] **Step 6: Run the logging test to verify it passes**

Run: `./dev cargo test -p tddy-build --test logging`
Expected: PASS.

- [ ] **Step 7: Run the full tddy-build suite (no regressions)**

Run: `./dev cargo test -p tddy-build`
Expected: PASS (existing `build_acceptance.rs` etc. unaffected).

- [ ] **Step 8: Commit**

```bash
git add packages/tddy-build/src/discovery.rs packages/tddy-build/src/lower.rs \
        packages/tddy-build/src/graph.rs packages/tddy-build/src/executor.rs \
        packages/tddy-build/src/cache.rs
git commit -m "feat(tddy-build): instrument engine with log:: at discovery/lower/graph/cache/exec seams"
```

---

## Task 3: Shared inputs/outputs helper in tddy-build

**Files:**
- Create: `packages/tddy-build/src/io.rs`
- Modify: `packages/tddy-build/src/lib.rs` (add `pub mod io;` and re-exports)
- Test: unit tests inside `io.rs`

- [ ] **Step 1: Write the failing test (in `io.rs`)**

```rust
//! Helpers letting recipe plugins declare cacheable inputs/outputs in open config.

use serde::Deserialize;

use crate::error::BuildError;
use crate::proto::{FileSet, OutputDecl, OutputKind};

/// A declared output in a plugin's open config: `{ path, kind }` where `kind` is
/// `file` (default) or `directory`/`dir`. Paths are repo-root-relative.
#[derive(Debug, Clone, Deserialize)]
pub struct OutputSpec {
    pub path: String,
    #[serde(default = "default_output_kind")]
    pub kind: String,
}

fn default_output_kind() -> String {
    "file".to_string()
}

/// Wrap `srcs` glob patterns into a single input [`FileSet`] rooted at `root`
/// (repo-root-relative; empty = repo root). Returns empty when there are no srcs.
pub fn srcs_to_inputs(srcs: &[String], root: &str) -> Vec<FileSet> {
    if srcs.is_empty() {
        return Vec::new();
    }
    vec![FileSet {
        include: srcs.to_vec(),
        exclude: Vec::new(),
        root: root.to_string(),
    }]
}

/// Convert declared [`OutputSpec`]s into proto [`OutputDecl`]s, validating `kind`.
pub fn outputs_to_decls(outputs: &[OutputSpec]) -> Result<Vec<OutputDecl>, BuildError> {
    outputs
        .iter()
        .map(|o| {
            let kind = match o.kind.as_str() {
                "file" => OutputKind::File,
                "directory" | "dir" => OutputKind::Directory,
                other => {
                    return Err(BuildError::Manifest(format!(
                        "invalid output kind {other:?} (expected file|directory)"
                    )))
                }
            };
            Ok(OutputDecl {
                path: o.path.clone(),
                kind: kind as i32,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srcs_become_one_rooted_fileset() {
        let sets = srcs_to_inputs(&["src/lib.rs".into(), "Cargo.toml".into()], "crate");
        assert_eq!(sets.len(), 1);
        assert_eq!(sets[0].include, vec!["src/lib.rs", "Cargo.toml"]);
        assert_eq!(sets[0].root, "crate");
    }

    #[test]
    fn empty_srcs_make_no_inputs() {
        assert!(srcs_to_inputs(&[], "").is_empty());
    }

    #[test]
    fn output_kinds_map_to_proto_and_default_to_file() {
        let specs = vec![
            OutputSpec {
                path: "bin/app".into(),
                kind: "file".into(),
            },
            OutputSpec {
                path: "dist".into(),
                kind: "directory".into(),
            },
        ];
        let decls = outputs_to_decls(&specs).expect("valid kinds");
        assert_eq!(decls[0].kind, OutputKind::File as i32);
        assert_eq!(decls[0].path, "bin/app");
        assert_eq!(decls[1].kind, OutputKind::Directory as i32);
    }

    #[test]
    fn invalid_output_kind_errors() {
        let specs = vec![OutputSpec {
            path: "x".into(),
            kind: "blob".into(),
        }];
        assert!(matches!(
            outputs_to_decls(&specs),
            Err(BuildError::Manifest(_))
        ));
    }
}
```

- [ ] **Step 2: Wire the module into `lib.rs`**

Add `pub mod io;` alongside the other `pub mod` lines, and extend the re-export:

```rust
pub use io::{outputs_to_decls, srcs_to_inputs, OutputSpec};
```

- [ ] **Step 3: Run the tests to verify they pass**

Run: `./dev cargo test -p tddy-build io::`
Expected: PASS (4 unit tests).

- [ ] **Step 4: Commit**

```bash
git add packages/tddy-build/src/io.rs packages/tddy-build/src/lib.rs
git commit -m "feat(tddy-build): add io helper for plugin-declared inputs/outputs"
```

---

## Task 4: Rust plugin emits inputs/outputs

**Files:**
- Modify: `packages/tddy-build-rust/src/lib.rs`

Add `srcs`/`outputs`/`working_dir` to both config structs and attach them to the lowered action.

- [ ] **Step 1: Write the failing test (append to the `tests` module in `lib.rs`)**

```rust
    #[test]
    fn rust_binary_emits_declared_inputs_and_outputs() {
        let config: serde_yaml::Value = serde_yaml::from_str(
            "package: mathapp\nbin_name: mathapp\nprofile: debug\n\
             srcs: [\"mathapp/src/main.rs\", \"mathapp/Cargo.toml\"]\n\
             outputs:\n  - path: \"target/debug/mathapp\"\n    kind: file\n",
        )
        .expect("valid yaml");
        let ctx = LowerContext {
            type_name: "rust_binary",
            target_id: "t",
            target_name: "",
            deps: &[],
            config: &config,
        };
        let actions = RustPlugin.lower(&ctx).expect("lower");
        let action = &actions[0];
        assert_eq!(action.command, vec!["cargo", "build", "-p", "mathapp", "--bin", "mathapp"]);
        assert_eq!(action.inputs.len(), 1);
        assert_eq!(action.inputs[0].include, vec!["mathapp/src/main.rs", "mathapp/Cargo.toml"]);
        assert_eq!(action.outputs.len(), 1);
        assert_eq!(action.outputs[0].path, "target/debug/mathapp");
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `./dev cargo test -p tddy-build-rust rust_binary_emits_declared`
Expected: FAIL — `srcs`/`outputs` rejected by `deny_unknown_fields` (or `inputs`/`outputs` empty).

- [ ] **Step 3: Implement**

Add fields to both structs:

```rust
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct RustBinary {
    package: String,
    bin_name: String,
    features: Vec<String>,
    profile: String,
    target_triple: String,
    srcs: Vec<String>,
    outputs: Vec<tddy_build::OutputSpec>,
    working_dir: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct RustLibrary {
    package: String,
    features: Vec<String>,
    profile: String,
    srcs: Vec<String>,
    outputs: Vec<tddy_build::OutputSpec>,
    working_dir: String,
}
```

Change the `lower` flow so the parsed config (not just the argv) reaches action assembly. Replace `rust_binary_action`/`rust_library_action` to also attach io, and capture `srcs`/`outputs`/`working_dir`:

```rust
    fn lower(&self, ctx: &LowerContext) -> Result<Vec<BuildAction>, BuildError> {
        let action = match ctx.type_name {
            "rust_binary" => {
                let rb: RustBinary = parse(ctx)?;
                rust_binary_action(rb)?
            }
            "rust_library" => {
                let rl: RustLibrary = parse(ctx)?;
                rust_library_action(rl)?
            }
            other => {
                return Err(BuildError::Manifest(format!(
                    "tddy-build-rust does not handle target type {other}"
                )))
            }
        };
        Ok(vec![action])
    }
```

```rust
fn rust_binary_action(rb: RustBinary) -> Result<BuildAction, BuildError> {
    let description = format!("cargo build {}", rb.package);
    let mut command = vec!["cargo".into(), "build".into(), "-p".into(), rb.package];
    if !rb.bin_name.is_empty() {
        command.push("--bin".into());
        command.push(rb.bin_name);
    }
    push_features(&mut command, rb.features);
    push_profile(&mut command, &rb.profile);
    if !rb.target_triple.is_empty() {
        command.push("--target".into());
        command.push(rb.target_triple);
    }
    finish("rust-binary", description, command, rb.srcs, rb.outputs, rb.working_dir)
}

fn rust_library_action(rl: RustLibrary) -> Result<BuildAction, BuildError> {
    let description = format!("cargo build {}", rl.package);
    let mut command = vec!["cargo".into(), "build".into(), "-p".into(), rl.package];
    push_features(&mut command, rl.features);
    push_profile(&mut command, &rl.profile);
    finish("rust-library", description, command, rl.srcs, rl.outputs, rl.working_dir)
}

fn finish(
    id: &str,
    description: String,
    command: Vec<String>,
    srcs: Vec<String>,
    outputs: Vec<tddy_build::OutputSpec>,
    working_dir: String,
) -> Result<BuildAction, BuildError> {
    Ok(BuildAction {
        id: id.to_string(),
        description,
        r#type: ActionType::Command as i32,
        command,
        inputs: tddy_build::srcs_to_inputs(&srcs, ""),
        outputs: tddy_build::outputs_to_decls(&outputs)?,
        working_dir,
        ..Default::default()
    })
}
```

Remove the now-unused `command_action` helper. Update the existing `lower` test helper in the `tests` module (it calls `RustPlugin.lower` and indexes `[0].command`) — it still works unchanged.

- [ ] **Step 4: Run to verify pass (new + existing rust plugin tests)**

Run: `./dev cargo test -p tddy-build-rust`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add packages/tddy-build-rust/src/lib.rs
git commit -m "feat(tddy-build-rust): emit declared srcs/outputs + working_dir on lowered action"
```

---

## Task 5: TypeScript plugin emits inputs/outputs

**Files:**
- Modify: `packages/tddy-build-typescript/src/lib.rs`

Add `srcs`; wire the existing `output_dirs` (currently dead) into directory outputs, joined onto `package_dir`. Inputs are rooted at `package_dir`.

- [ ] **Step 1: Write the failing test (append to `tests` module)**

```rust
    #[test]
    fn typescript_emits_srcs_rooted_at_package_dir_and_output_dirs() {
        let action = lower(
            "package_dir: packages/shared\nbuild_script: build\n\
             srcs: [\"src/index.ts\", \"package.json\"]\noutput_dirs: [dist]\n",
        );
        assert_eq!(action.command, vec!["bun", "run", "build"]);
        assert_eq!(action.working_dir, "packages/shared");
        assert_eq!(action.inputs.len(), 1);
        assert_eq!(action.inputs[0].root, "packages/shared");
        assert_eq!(action.inputs[0].include, vec!["src/index.ts", "package.json"]);
        assert_eq!(action.outputs.len(), 1);
        assert_eq!(action.outputs[0].path, "packages/shared/dist");
        assert_eq!(action.outputs[0].kind, tddy_build::OutputKind::Directory as i32);
    }
```

> Note: the existing `lower` test helper returns a `BuildAction`. Keep it as-is.

- [ ] **Step 2: Run to verify it fails**

Run: `./dev cargo test -p tddy-build-typescript typescript_emits_srcs`
Expected: FAIL — `srcs` unknown field / outputs empty.

- [ ] **Step 3: Implement**

```rust
use tddy_build::proto::{ActionType, BuildAction, OutputDecl, OutputKind};

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct TypeScript {
    package_dir: String,
    build_script: String,
    srcs: Vec<String>,
    output_dirs: Vec<String>,
}

impl BuildPlugin for TypeScriptPlugin {
    fn type_names(&self) -> &'static [&'static str] {
        &["typescript"]
    }

    fn lower(&self, ctx: &LowerContext) -> Result<Vec<BuildAction>, BuildError> {
        let ts: TypeScript = serde_yaml::from_value(ctx.config.clone())
            .map_err(|e| BuildError::Manifest(format!("invalid typescript config: {e}")))?;

        let script = if ts.build_script.is_empty() {
            "build".to_string()
        } else {
            ts.build_script
        };
        let outputs: Vec<OutputDecl> = ts
            .output_dirs
            .iter()
            .map(|d| OutputDecl {
                path: join_pkg(&ts.package_dir, d),
                kind: OutputKind::Directory as i32,
            })
            .collect();
        Ok(vec![BuildAction {
            id: "typescript".to_string(),
            description: format!("bun run {script}"),
            r#type: ActionType::Command as i32,
            command: vec!["bun".to_string(), "run".to_string(), script],
            inputs: tddy_build::srcs_to_inputs(&ts.srcs, &ts.package_dir),
            outputs,
            working_dir: ts.package_dir,
            ..Default::default()
        }])
    }
}

fn join_pkg(package_dir: &str, dir: &str) -> String {
    if package_dir.is_empty() {
        dir.to_string()
    } else {
        format!("{}/{}", package_dir.trim_end_matches('/'), dir)
    }
}
```

- [ ] **Step 4: Run to verify pass**

Run: `./dev cargo test -p tddy-build-typescript`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add packages/tddy-build-typescript/src/lib.rs
git commit -m "feat(tddy-build-typescript): emit srcs inputs + wire output_dirs into outputs"
```

---

## Task 6: Docker plugin emits inputs/outputs (+ --iidfile)

**Files:**
- Modify: `packages/tddy-build-docker/src/lib.rs`

Add `srcs` (context/Dockerfile inputs) and `outputs`; when an output is declared, add `--iidfile <outputs[0].path>` so docker writes the image id to a real file the cache can fingerprint.

- [ ] **Step 1: Write the failing test (append to `tests` module)**

```rust
    #[test]
    fn docker_emits_iidfile_inputs_and_outputs() {
        let config: serde_yaml::Value = serde_yaml::from_str(
            "tag: example-base\ndockerfile: base/Dockerfile\ncontext: base\n\
             srcs: [\"base/Dockerfile\"]\n\
             outputs:\n  - path: \".tddy-build/iid/base.txt\"\n    kind: file\n",
        )
        .expect("valid yaml");
        let ctx = LowerContext {
            type_name: "docker_image",
            target_id: "t",
            target_name: "",
            deps: &[],
            config: &config,
        };
        let action = DockerPlugin.lower(&ctx).expect("lower").remove(0);
        assert_eq!(
            action.command,
            vec![
                "docker", "build", "-f", "base/Dockerfile", "-t", "example-base",
                "--iidfile", ".tddy-build/iid/base.txt", "base"
            ]
        );
        assert_eq!(action.inputs[0].include, vec!["base/Dockerfile"]);
        assert_eq!(action.outputs[0].path, ".tddy-build/iid/base.txt");
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `./dev cargo test -p tddy-build-docker docker_emits_iidfile`
Expected: FAIL.

- [ ] **Step 3: Implement**

```rust
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct DockerImage {
    dockerfile: String,
    context: String,
    tag: String,
    build_args: Vec<String>,
    srcs: Vec<String>,
    outputs: Vec<tddy_build::OutputSpec>,
}

impl BuildPlugin for DockerPlugin {
    fn type_names(&self) -> &'static [&'static str] {
        &["docker_image"]
    }

    fn lower(&self, ctx: &LowerContext) -> Result<Vec<BuildAction>, BuildError> {
        let d: DockerImage = serde_yaml::from_value(ctx.config.clone())
            .map_err(|e| BuildError::Manifest(format!("invalid docker_image config: {e}")))?;

        let description = format!("docker build {}", d.tag);
        let mut command = vec!["docker".to_string(), "build".to_string()];
        if !d.dockerfile.is_empty() {
            command.push("-f".to_string());
            command.push(d.dockerfile);
        }
        if !d.tag.is_empty() {
            command.push("-t".to_string());
            command.push(d.tag);
        }
        for arg in d.build_args {
            command.push("--build-arg".to_string());
            command.push(arg);
        }
        let outputs = tddy_build::outputs_to_decls(&d.outputs)?;
        if let Some(first) = outputs.first() {
            command.push("--iidfile".to_string());
            command.push(first.path.clone());
        }
        command.push(if d.context.is_empty() {
            ".".to_string()
        } else {
            d.context
        });

        Ok(vec![BuildAction {
            id: "docker-image".to_string(),
            description,
            r#type: ActionType::Command as i32,
            command,
            inputs: tddy_build::srcs_to_inputs(&d.srcs, ""),
            outputs,
            ..Default::default()
        }])
    }
}
```

- [ ] **Step 4: Run to verify pass**

Run: `./dev cargo test -p tddy-build-docker`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add packages/tddy-build-docker/src/lib.rs
git commit -m "feat(tddy-build-docker): emit srcs inputs + --iidfile output for caching"
```

---

## Task 7: Engine example — runnable script/tool/group pipeline

**Files:**
- Create: `packages/tddy-build/examples/pipeline/codegen/BUILD.yaml`
- Create: `packages/tddy-build/examples/pipeline/lib/BUILD.yaml`
- Create: `packages/tddy-build/examples/pipeline/app/BUILD.yaml`
- Create: `packages/tddy-build/examples/pipeline/tools/bin/stamp` (executable shell stub)
- Create: `packages/tddy-build/tests/example_pipeline.rs`

The pipeline: `codegen:gen` (script) writes `generated.txt`; `lib:build` consumes it and writes `lib.txt`; `app:build` consumes `lib.txt` and uses a `tool`-provided `stamp` binary on PATH. `pipeline:all` groups them. All built-in types — no toolchain needed, fully runnable.

- [ ] **Step 1: Create the example manifests and tool stub**

`codegen/BUILD.yaml`:

```yaml
schema_version: 1
targets:
  - id: "codegen:gen"
    name: "Generate sources"
    actions:
      - id: "gen"
        type: command
        command: ["sh", "-c", "echo generated > generated.txt"]
        inputs:
          - include: ["codegen/seed.txt"]
            root: "."
        outputs:
          - path: "generated.txt"
            kind: file
```

`lib/BUILD.yaml`:

```yaml
schema_version: 1
targets:
  - id: "lib:build"
    name: "Build lib"
    deps: ["codegen:gen"]
    actions:
      - id: "build"
        type: command
        command: ["sh", "-c", "cat generated.txt > lib.txt"]
        inputs:
          - include: ["generated.txt"]
            root: "."
        outputs:
          - path: "lib.txt"
            kind: file
```

`app/BUILD.yaml`:

```yaml
schema_version: 1
targets:
  - id: "tools:bin"
    name: "Tools"
    config:
      type: tool
      bin_dir: tools/bin
      commands: { stamp: stamp }
  - id: "app:build"
    name: "Build app"
    deps: ["lib:build"]
    actions:
      - id: "package"
        type: command
        command: ["sh", "-c", "stamp < lib.txt > app.txt"]
        tool_dep_ids: ["tools:bin"]
        inputs:
          - include: ["lib.txt"]
            root: "."
        outputs:
          - path: "app.txt"
            kind: file
  - id: "pipeline:all"
    name: "Whole pipeline"
    config:
      type: group
      member_ids: ["codegen:gen", "lib:build", "app:build"]
```

`tools/bin/stamp` (make executable in Step 2):

```sh
#!/bin/sh
echo "STAMPED:"
cat
```

- [ ] **Step 2: Make the stub executable and seed the input**

```bash
chmod +x packages/tddy-build/examples/pipeline/tools/bin/stamp
printf 'seed\n' > packages/tddy-build/examples/pipeline/codegen/seed.txt
```

- [ ] **Step 3: Write the integration test**

```rust
//! Exercises the engine's built-in script/tool/group types as a real, runnable
//! multi-package pipeline: discovery, cross-package deps, real execution, the action
//! cache (hit on rerun, miss on input edit), and cycle detection.

use std::path::PathBuf;

use tddy_build::discovery::discover_build_manifests;
use tddy_build::executor::{execute_target, ExecuteOptions};
use tddy_build::graph::BuildGraph;
use tddy_build::plugin::PluginRegistry;

fn example_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/pipeline")
}

/// Copy the committed example into a fresh tempdir so builds never dirty the repo.
fn staged() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    copy_dir(&example_root(), dir.path());
    dir
}

fn copy_dir(src: &std::path::Path, dst: &std::path::Path) {
    for entry in std::fs::read_dir(src).expect("read_dir") {
        let entry = entry.expect("entry");
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            std::fs::create_dir_all(&to).expect("mkdir");
            copy_dir(&from, &to);
        } else {
            std::fs::copy(&from, &to).expect("copy");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = std::fs::metadata(&from).unwrap().permissions().mode();
                std::fs::set_permissions(&to, std::fs::Permissions::from_mode(mode)).unwrap();
            }
        }
    }
}

fn load(root: &std::path::Path) -> BuildGraph {
    let manifests = discover_build_manifests(root)
        .expect("discover")
        .into_iter()
        .map(|(_, m)| m)
        .collect();
    BuildGraph::from_manifests(manifests).expect("graph")
}

#[test]
fn targets_reference_each_other_and_resolve_deps_first() {
    let graph = load(&example_root());
    let order = graph.build_order("app:build").expect("build order");
    let pos = |id: &str| order.iter().position(|t| t == id).expect("present");
    assert!(pos("codegen:gen") < pos("lib:build"));
    assert!(pos("lib:build") < pos("app:build"));
}

#[tokio::test]
async fn pipeline_builds_successfully_through_a_tool_target() {
    let dir = staged();
    let graph = load(dir.path());
    let record = execute_target(
        dir.path(),
        &graph,
        "app:build",
        &ExecuteOptions::default(),
        &PluginRegistry::new(),
    )
    .await
    .expect("build app");
    assert_eq!(record.actions[0].exit_code, 0);
    let app = std::fs::read_to_string(dir.path().join("app.txt")).expect("app.txt");
    assert!(app.contains("STAMPED:"), "tool stub must run, got: {app:?}");
    assert!(app.contains("generated"), "pipeline output threads through, got: {app:?}");
}

#[tokio::test]
async fn cache_hits_on_rerun_and_misses_after_input_edit() {
    let dir = staged();
    let opts = ExecuteOptions::default();
    let registry = PluginRegistry::new();

    let graph = load(dir.path());
    let first = execute_target(dir.path(), &graph, "codegen:gen", &opts, &registry)
        .await
        .expect("first");
    assert!(!first.actions[0].cached, "first run executes");

    let second = execute_target(dir.path(), &graph, "codegen:gen", &opts, &registry)
        .await
        .expect("second");
    assert!(second.actions[0].cached, "rerun is a cache hit");

    // Edit a declared input → fingerprint changes → miss.
    std::fs::write(dir.path().join("codegen/seed.txt"), "seed-changed").expect("edit seed");
    let third = execute_target(dir.path(), &graph, "codegen:gen", &opts, &registry)
        .await
        .expect("third");
    assert!(!third.actions[0].cached, "edited input invalidates the cache");
}

#[test]
fn group_membership_orders_the_whole_pipeline() {
    let graph = load(&example_root());
    let order = graph.build_order("pipeline:all").expect("group order");
    for member in ["codegen:gen", "lib:build", "app:build"] {
        assert!(order.contains(&member.to_string()), "{member} in group order");
    }
}

#[test]
fn engine_detects_self_loop_and_multi_node_cycles() {
    // Self-loop: a target depending on itself.
    let self_loop = r#"
schema_version: 1
targets:
  - id: "a:t"
    name: A
    deps: ["a:t"]
    config: { type: script, command: ["true"] }
"#;
    let m = tddy_build::load_build_manifest(self_loop).expect("parse self loop");
    assert!(BuildGraph::from_manifests(vec![m]).is_err(), "self-loop is a cycle");

    // Three-node cycle: a -> b -> c -> a.
    let three = r#"
schema_version: 1
targets:
  - id: "a:t"
    name: A
    deps: ["b:t"]
    config: { type: script, command: ["true"] }
  - id: "b:t"
    name: B
    deps: ["c:t"]
    config: { type: script, command: ["true"] }
  - id: "c:t"
    name: C
    deps: ["a:t"]
    config: { type: script, command: ["true"] }
"#;
    let m = tddy_build::load_build_manifest(three).expect("parse 3-cycle");
    assert!(BuildGraph::from_manifests(vec![m]).is_err(), "3-node cycle is rejected");
}
```

- [ ] **Step 4: Run the tests**

Run: `./dev cargo test -p tddy-build --test example_pipeline`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add packages/tddy-build/examples packages/tddy-build/tests/example_pipeline.rs
git commit -m "test(tddy-build): runnable script/tool/group pipeline example + verification"
```

---

## Task 8: Rust example — real cargo workspace

**Files:**
- Create: `packages/tddy-build-rust/examples/workspace/Cargo.toml` (own `[workspace]`)
- Create: `packages/tddy-build-rust/examples/workspace/mathcore/{Cargo.toml,src/lib.rs}`
- Create: `packages/tddy-build-rust/examples/workspace/mathutil/{Cargo.toml,src/lib.rs}`
- Create: `packages/tddy-build-rust/examples/workspace/mathapp/{Cargo.toml,src/main.rs}`
- Create: `packages/tddy-build-rust/examples/workspace/{core,util,app}/BUILD.yaml`
- Create: `packages/tddy-build-rust/tests/example_workspace.rs`
- Modify: root `Cargo.toml` (add `exclude`)
- Modify: `packages/tddy-build-rust/Cargo.toml` (add `tempfile` + `tokio` dev-deps)

`mathcore` (lib) ← `mathutil` (lib, path-deps mathcore) ← `mathapp` (bin, path-deps both). The BUILD graph mirrors these with `deps`. Crate names are deliberately non-`core`/`std`-colliding.

- [ ] **Step 1: Create the cargo workspace**

`examples/workspace/Cargo.toml`:

```toml
[workspace]
resolver = "2"
members = ["mathcore", "mathutil", "mathapp"]
```

`mathcore/Cargo.toml`:

```toml
[package]
name = "mathcore"
version = "0.0.0"
edition = "2021"
publish = false

[lib]
path = "src/lib.rs"
```

`mathcore/src/lib.rs`:

```rust
pub fn add(a: i64, b: i64) -> i64 {
    a + b
}
```

`mathutil/Cargo.toml`:

```toml
[package]
name = "mathutil"
version = "0.0.0"
edition = "2021"
publish = false

[lib]
path = "src/lib.rs"

[dependencies]
mathcore = { path = "../mathcore" }
```

`mathutil/src/lib.rs`:

```rust
pub fn double_sum(a: i64, b: i64) -> i64 {
    mathcore::add(a, b) * 2
}
```

`mathapp/Cargo.toml`:

```toml
[package]
name = "mathapp"
version = "0.0.0"
edition = "2021"
publish = false

[[bin]]
name = "mathapp"
path = "src/main.rs"

[dependencies]
mathcore = { path = "../mathcore" }
mathutil = { path = "../mathutil" }
```

`mathapp/src/main.rs`:

```rust
fn main() {
    println!("{}", mathutil::double_sum(mathcore::add(1, 2), 3));
}
```

- [ ] **Step 2: Create the BUILD manifests**

`core/BUILD.yaml`:

```yaml
schema_version: 1
targets:
  - id: "mathcore:lib"
    name: "mathcore library"
    config:
      type: rust_library
      package: mathcore
      profile: debug
      srcs: ["mathcore/src/lib.rs", "mathcore/Cargo.toml"]
      outputs:
        - path: "target/debug/libmathcore.rlib"
          kind: file
```

`util/BUILD.yaml`:

```yaml
schema_version: 1
targets:
  - id: "mathutil:lib"
    name: "mathutil library"
    deps: ["mathcore:lib"]
    config:
      type: rust_library
      package: mathutil
      profile: debug
      srcs: ["mathutil/src/lib.rs", "mathutil/Cargo.toml"]
      outputs:
        - path: "target/debug/libmathutil.rlib"
          kind: file
```

`app/BUILD.yaml`:

```yaml
schema_version: 1
targets:
  - id: "mathapp:bin"
    name: "mathapp binary"
    deps: ["mathcore:lib", "mathutil:lib"]
    config:
      type: rust_binary
      package: mathapp
      bin_name: mathapp
      profile: debug
      srcs: ["mathapp/src/main.rs", "mathapp/Cargo.toml"]
      outputs:
        - path: "target/debug/mathapp"
          kind: file
```

- [ ] **Step 3: Exclude the example from the outer workspace and add the dev-dep**

In root `Cargo.toml`, add after the `members = [...]` array:

```toml
exclude = [
    "packages/tddy-build-rust/examples/workspace",
]
```

In `packages/tddy-build-rust/Cargo.toml`, add (create the section if absent):

```toml
[dev-dependencies]
tempfile = "3"
tokio = { version = "1", features = ["rt", "rt-multi-thread", "macros"] }
```

- [ ] **Step 4: Write the integration test**

```rust
//! Exercises the rust recipe plugin on a real, interdependent cargo workspace:
//! deps-first ordering, real `cargo build`, and the action cache (hit on rerun,
//! miss after a source edit). Skips with a notice when cargo is unavailable.

use std::path::PathBuf;
use std::sync::Arc;

use tddy_build::discovery::discover_build_manifests;
use tddy_build::executor::{execute_target, ExecuteOptions};
use tddy_build::graph::BuildGraph;
use tddy_build::plugin::PluginRegistry;
use tddy_build_rust::RustPlugin;

fn example_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/workspace")
}

fn cargo_available() -> bool {
    std::process::Command::new("cargo")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn registry() -> PluginRegistry {
    let mut r = PluginRegistry::new();
    r.register(Arc::new(RustPlugin));
    r
}

fn staged() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    copy_dir(&example_root(), dir.path());
    dir
}

fn copy_dir(src: &std::path::Path, dst: &std::path::Path) {
    for entry in std::fs::read_dir(src).expect("read_dir") {
        let entry = entry.expect("entry");
        let from = entry.path();
        if from.file_name().map(|n| n == "target").unwrap_or(false) {
            continue; // never copy build artifacts
        }
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            std::fs::create_dir_all(&to).expect("mkdir");
            copy_dir(&from, &to);
        } else {
            std::fs::copy(&from, &to).expect("copy");
        }
    }
}

fn load(root: &std::path::Path) -> BuildGraph {
    let manifests = discover_build_manifests(root)
        .expect("discover")
        .into_iter()
        .map(|(_, m)| m)
        .collect();
    BuildGraph::from_manifests(manifests).expect("graph")
}

#[test]
fn rust_targets_depend_on_each_other_deps_first() {
    let graph = load(&example_root());
    let order = graph.build_order("mathapp:bin").expect("order");
    let pos = |id: &str| order.iter().position(|t| t == id).expect("present");
    assert!(pos("mathcore:lib") < pos("mathutil:lib"));
    assert!(pos("mathutil:lib") < pos("mathapp:bin"));
}

#[test]
fn rust_plugin_lowers_expected_cargo_argv() {
    let graph = load(&example_root());
    let actions = graph.actions_for("mathapp:bin", &registry()).expect("lower");
    assert_eq!(
        actions[0].command,
        vec!["cargo", "build", "-p", "mathapp", "--bin", "mathapp"]
    );
}

#[tokio::test]
async fn rust_workspace_builds_with_real_cargo() {
    if !cargo_available() {
        eprintln!("SKIP: cargo not available");
        return;
    }
    let dir = staged();
    let graph = load(dir.path());
    let record = execute_target(
        dir.path(),
        &graph,
        "mathapp:bin",
        &ExecuteOptions::default(),
        &registry(),
    )
    .await
    .expect("cargo build");
    assert_eq!(record.actions[0].exit_code, 0, "stderr: {}", record.actions[0].stderr);
    assert!(dir.path().join("target/debug/mathapp").exists(), "binary produced");
}

#[tokio::test]
async fn rust_cache_hits_then_misses_after_source_edit() {
    if !cargo_available() {
        eprintln!("SKIP: cargo not available");
        return;
    }
    let dir = staged();
    let opts = ExecuteOptions::default();
    let reg = registry();
    let graph = load(dir.path());

    let first = execute_target(dir.path(), &graph, "mathcore:lib", &opts, &reg)
        .await
        .expect("first");
    assert!(!first.actions[0].cached);

    let second = execute_target(dir.path(), &graph, "mathcore:lib", &opts, &reg)
        .await
        .expect("second");
    assert!(second.actions[0].cached, "rerun is a cache hit");

    std::fs::write(
        dir.path().join("mathcore/src/lib.rs"),
        "pub fn add(a: i64, b: i64) -> i64 { a + b + 0 }\n",
    )
    .expect("edit source");
    let third = execute_target(dir.path(), &graph, "mathcore:lib", &opts, &reg)
        .await
        .expect("third");
    assert!(!third.actions[0].cached, "source edit invalidates the cache");
}

#[test]
fn rust_typed_cycle_is_detected() {
    let yaml = r#"
schema_version: 1
targets:
  - id: "x:lib"
    name: X
    deps: ["y:lib"]
    config: { type: rust_library, package: x }
  - id: "y:lib"
    name: Y
    deps: ["x:lib"]
    config: { type: rust_library, package: y }
"#;
    let manifest = tddy_build::load_build_manifest(yaml).expect("parse");
    assert!(
        BuildGraph::from_manifests(vec![manifest]).is_err(),
        "a cycle between plugin-typed targets must be rejected"
    );
}
```

- [ ] **Step 5: Verify the example workspace is isolated, then run tests**

Run: `./dev cargo build -p tddy-build-rust` (must still build; the example is excluded, not a member).
Run: `./dev cargo test -p tddy-build-rust --test example_workspace`
Expected: PASS. The two `#[tokio::test]`s run real cargo (a few seconds); if cargo were missing they'd print `SKIP`.

- [ ] **Step 6: Commit**

```bash
git add packages/tddy-build-rust/examples packages/tddy-build-rust/tests/example_workspace.rs \
        packages/tddy-build-rust/Cargo.toml Cargo.toml
git commit -m "test(tddy-build-rust): real cargo workspace example + deps/cache/cycle verification"
```

---

## Task 9: TypeScript example — real bun monorepo

**Files:**
- Create: `packages/tddy-build-typescript/examples/monorepo/packages/shared/{package.json,src/index.ts}`
- Create: `packages/tddy-build-typescript/examples/monorepo/packages/ui/{package.json,src/index.ts}`
- Create: `packages/tddy-build-typescript/examples/monorepo/apps/web/{package.json,src/index.ts}`
- Create: `packages/tddy-build-typescript/examples/monorepo/packages/shared/BUILD.yaml`
- Create: `packages/tddy-build-typescript/examples/monorepo/packages/ui/BUILD.yaml`
- Create: `packages/tddy-build-typescript/examples/monorepo/apps/web/BUILD.yaml`
- Create: `packages/tddy-build-typescript/tests/example_monorepo.rs`
- Modify: `packages/tddy-build-typescript/Cargo.toml` (add `tempfile` + `tokio` dev-deps)

Each package is self-contained (no workspace, no install): a `build` script runs
`bun build ./src/index.ts --outdir dist`. The BUILD manifests live inside each
package dir so discovery finds them; `deps` mirror shared ← ui ← web.

- [ ] **Step 1: Create the bun packages**

`packages/shared/package.json`:

```json
{
  "name": "shared",
  "version": "0.0.0",
  "private": true,
  "scripts": { "build": "bun build ./src/index.ts --outdir dist" }
}
```

`packages/shared/src/index.ts`:

```ts
export const greeting = "hello from shared";
```

`packages/ui/package.json`:

```json
{
  "name": "ui",
  "version": "0.0.0",
  "private": true,
  "scripts": { "build": "bun build ./src/index.ts --outdir dist" }
}
```

`packages/ui/src/index.ts`:

```ts
export const button = () => "ui-button";
```

`apps/web/package.json`:

```json
{
  "name": "web",
  "version": "0.0.0",
  "private": true,
  "scripts": { "build": "bun build ./src/index.ts --outdir dist" }
}
```

`apps/web/src/index.ts`:

```ts
export const page = () => "web-page";
```

- [ ] **Step 2: Create the BUILD manifests (one per package dir)**

`packages/shared/BUILD.yaml`:

```yaml
schema_version: 1
targets:
  - id: "shared:build"
    name: "Build shared"
    config:
      type: typescript
      package_dir: packages/shared
      build_script: build
      srcs: ["src/index.ts", "package.json"]
      output_dirs: ["dist"]
```

`packages/ui/BUILD.yaml`:

```yaml
schema_version: 1
targets:
  - id: "ui:build"
    name: "Build ui"
    deps: ["shared:build"]
    config:
      type: typescript
      package_dir: packages/ui
      build_script: build
      srcs: ["src/index.ts", "package.json"]
      output_dirs: ["dist"]
```

`apps/web/BUILD.yaml`:

```yaml
schema_version: 1
targets:
  - id: "web:build"
    name: "Build web"
    deps: ["shared:build", "ui:build"]
    config:
      type: typescript
      package_dir: apps/web
      build_script: build
      srcs: ["src/index.ts", "package.json"]
      output_dirs: ["dist"]
```

- [ ] **Step 3: Add the dev-dep**

In `packages/tddy-build-typescript/Cargo.toml` add (create section if absent):

```toml
[dev-dependencies]
tempfile = "3"
tokio = { version = "1", features = ["rt", "rt-multi-thread", "macros"] }
```

- [ ] **Step 4: Write the integration test**

```rust
//! Exercises the typescript recipe plugin on a real, interdependent bun monorepo:
//! deps-first ordering, real `bun run build`, and the action cache. Skips when bun
//! is unavailable.

use std::path::PathBuf;
use std::sync::Arc;

use tddy_build::discovery::discover_build_manifests;
use tddy_build::executor::{execute_target, ExecuteOptions};
use tddy_build::graph::BuildGraph;
use tddy_build::plugin::PluginRegistry;
use tddy_build_typescript::TypeScriptPlugin;

fn example_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/monorepo")
}

fn bun_available() -> bool {
    std::process::Command::new("bun")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn registry() -> PluginRegistry {
    let mut r = PluginRegistry::new();
    r.register(Arc::new(TypeScriptPlugin));
    r
}

fn staged() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    copy_dir(&example_root(), dir.path());
    dir
}

fn copy_dir(src: &std::path::Path, dst: &std::path::Path) {
    for entry in std::fs::read_dir(src).expect("read_dir") {
        let entry = entry.expect("entry");
        let from = entry.path();
        let name = entry.file_name();
        if name == "dist" || name == "node_modules" {
            continue;
        }
        let to = dst.join(&name);
        if from.is_dir() {
            std::fs::create_dir_all(&to).expect("mkdir");
            copy_dir(&from, &to);
        } else {
            std::fs::copy(&from, &to).expect("copy");
        }
    }
}

fn load(root: &std::path::Path) -> BuildGraph {
    let manifests = discover_build_manifests(root)
        .expect("discover")
        .into_iter()
        .map(|(_, m)| m)
        .collect();
    BuildGraph::from_manifests(manifests).expect("graph")
}

#[test]
fn ts_targets_depend_on_each_other_deps_first() {
    let graph = load(&example_root());
    let order = graph.build_order("web:build").expect("order");
    let pos = |id: &str| order.iter().position(|t| t == id).expect("present");
    assert!(pos("shared:build") < pos("ui:build"));
    assert!(pos("ui:build") < pos("web:build"));
}

#[test]
fn ts_plugin_lowers_expected_bun_argv_and_workdir() {
    let graph = load(&example_root());
    let actions = graph.actions_for("shared:build", &registry()).expect("lower");
    assert_eq!(actions[0].command, vec!["bun", "run", "build"]);
    assert_eq!(actions[0].working_dir, "packages/shared");
}

#[tokio::test]
async fn ts_monorepo_builds_with_real_bun() {
    if !bun_available() {
        eprintln!("SKIP: bun not available");
        return;
    }
    let dir = staged();
    let graph = load(dir.path());
    let record = execute_target(
        dir.path(),
        &graph,
        "web:build",
        &ExecuteOptions::default(),
        &registry(),
    )
    .await
    .expect("bun build");
    assert_eq!(record.actions[0].exit_code, 0, "stderr: {}", record.actions[0].stderr);
    assert!(dir.path().join("apps/web/dist").exists(), "dist produced");
}

#[tokio::test]
async fn ts_cache_hits_then_misses_after_source_edit() {
    if !bun_available() {
        eprintln!("SKIP: bun not available");
        return;
    }
    let dir = staged();
    let opts = ExecuteOptions::default();
    let reg = registry();
    let graph = load(dir.path());

    let first = execute_target(dir.path(), &graph, "shared:build", &opts, &reg)
        .await
        .expect("first");
    assert!(!first.actions[0].cached);

    let second = execute_target(dir.path(), &graph, "shared:build", &opts, &reg)
        .await
        .expect("second");
    assert!(second.actions[0].cached, "rerun is a cache hit");

    std::fs::write(
        dir.path().join("packages/shared/src/index.ts"),
        "export const greeting = \"hello again\";\n",
    )
    .expect("edit source");
    let third = execute_target(dir.path(), &graph, "shared:build", &opts, &reg)
        .await
        .expect("third");
    assert!(!third.actions[0].cached, "source edit invalidates the cache");
}
```

- [ ] **Step 5: Run the tests**

Run: `./dev cargo test -p tddy-build-typescript --test example_monorepo`
Expected: PASS (build/cache tests run real bun; SKIP if bun missing).

- [ ] **Step 6: Commit**

```bash
git add packages/tddy-build-typescript/examples packages/tddy-build-typescript/tests/example_monorepo.rs \
        packages/tddy-build-typescript/Cargo.toml
git commit -m "test(tddy-build-typescript): real bun monorepo example + deps/cache verification"
```

---

## Task 10: Docker example — real images (daemon-gated)

**Files:**
- Create: `packages/tddy-build-docker/examples/images/base/Dockerfile`
- Create: `packages/tddy-build-docker/examples/images/api/Dockerfile`
- Create: `packages/tddy-build-docker/examples/images/worker/Dockerfile`
- Create: `packages/tddy-build-docker/examples/images/{base,api,worker}/BUILD.yaml`
- Create: `packages/tddy-build-docker/tests/example_images.rs`
- Modify: `packages/tddy-build-docker/Cargo.toml` (add `tempfile` + `tokio` dev-deps)

`base` is an image; `api` and `worker` are `FROM example-base`. The BUILD graph
orders base before its dependents. Real `docker build` is gated on `docker info`.

- [ ] **Step 1: Create the Dockerfiles**

`base/Dockerfile`:

```dockerfile
FROM busybox:latest
RUN echo "base image" > /base.txt
```

`api/Dockerfile`:

```dockerfile
FROM example-base
RUN echo "api image" > /api.txt
```

`worker/Dockerfile`:

```dockerfile
FROM example-base
RUN echo "worker image" > /worker.txt
```

- [ ] **Step 2: Create the BUILD manifests**

`base/BUILD.yaml`:

```yaml
schema_version: 1
targets:
  - id: "base:image"
    name: "Base image"
    config:
      type: docker_image
      tag: example-base
      dockerfile: base/Dockerfile
      context: base
      srcs: ["base/Dockerfile"]
      outputs:
        - path: ".tddy-build/iid/base.txt"
          kind: file
```

`api/BUILD.yaml`:

```yaml
schema_version: 1
targets:
  - id: "api:image"
    name: "API image"
    deps: ["base:image"]
    config:
      type: docker_image
      tag: example-api
      dockerfile: api/Dockerfile
      context: api
      srcs: ["api/Dockerfile"]
      outputs:
        - path: ".tddy-build/iid/api.txt"
          kind: file
```

`worker/BUILD.yaml`:

```yaml
schema_version: 1
targets:
  - id: "worker:image"
    name: "Worker image"
    deps: ["base:image"]
    config:
      type: docker_image
      tag: example-worker
      dockerfile: worker/Dockerfile
      context: worker
      srcs: ["worker/Dockerfile"]
      outputs:
        - path: ".tddy-build/iid/worker.txt"
          kind: file
```

- [ ] **Step 3: Add the dev-dep**

In `packages/tddy-build-docker/Cargo.toml` add (create section if absent):

```toml
[dev-dependencies]
tempfile = "3"
tokio = { version = "1", features = ["rt", "rt-multi-thread", "macros"] }
```

- [ ] **Step 4: Write the integration test**

```rust
//! Exercises the docker recipe plugin on a real, interdependent image set:
//! deps-first ordering, lowered argv (incl. --iidfile), and — when a docker daemon
//! is reachable — real `docker build` plus action-cache hit/miss.

use std::path::PathBuf;
use std::sync::Arc;

use tddy_build::discovery::discover_build_manifests;
use tddy_build::executor::{execute_target, ExecuteOptions};
use tddy_build::graph::BuildGraph;
use tddy_build::plugin::PluginRegistry;
use tddy_build_docker::DockerPlugin;

fn example_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/images")
}

fn docker_up() -> bool {
    std::process::Command::new("docker")
        .arg("info")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn registry() -> PluginRegistry {
    let mut r = PluginRegistry::new();
    r.register(Arc::new(DockerPlugin));
    r
}

fn staged() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    copy_dir(&example_root(), dir.path());
    dir
}

fn copy_dir(src: &std::path::Path, dst: &std::path::Path) {
    for entry in std::fs::read_dir(src).expect("read_dir") {
        let entry = entry.expect("entry");
        let from = entry.path();
        if from.file_name().map(|n| n == ".tddy-build").unwrap_or(false) {
            continue;
        }
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            std::fs::create_dir_all(&to).expect("mkdir");
            copy_dir(&from, &to);
        } else {
            std::fs::copy(&from, &to).expect("copy");
        }
    }
}

fn load(root: &std::path::Path) -> BuildGraph {
    let manifests = discover_build_manifests(root)
        .expect("discover")
        .into_iter()
        .map(|(_, m)| m)
        .collect();
    BuildGraph::from_manifests(manifests).expect("graph")
}

#[test]
fn docker_targets_depend_on_base_first() {
    let graph = load(&example_root());
    let order = graph.build_order("api:image").expect("order");
    let pos = |id: &str| order.iter().position(|t| t == id).expect("present");
    assert!(pos("base:image") < pos("api:image"));
}

#[test]
fn docker_plugin_lowers_iidfile_argv() {
    let graph = load(&example_root());
    let actions = graph.actions_for("base:image", &registry()).expect("lower");
    assert_eq!(
        actions[0].command,
        vec![
            "docker", "build", "-f", "base/Dockerfile", "-t", "example-base",
            "--iidfile", ".tddy-build/iid/base.txt", "base"
        ]
    );
}

#[tokio::test]
async fn docker_images_build_and_cache_when_daemon_available() {
    if !docker_up() {
        eprintln!("SKIP: docker daemon not reachable");
        return;
    }
    let dir = staged();
    let opts = ExecuteOptions::default();
    let reg = registry();
    let graph = load(dir.path());

    // base + api build (deps-first); api is FROM example-base.
    let record = execute_target(dir.path(), &graph, "api:image", &opts, &reg)
        .await
        .expect("docker build");
    assert_eq!(record.actions[0].exit_code, 0, "stderr: {}", record.actions[0].stderr);
    assert!(dir.path().join(".tddy-build/iid/api.txt").exists(), "iidfile written");

    // Rerun base alone → cache hit.
    let second = execute_target(dir.path(), &graph, "base:image", &opts, &reg)
        .await
        .expect("second base");
    assert!(second.actions[0].cached, "rerun is a cache hit");

    // Edit the base Dockerfile → miss.
    std::fs::write(
        dir.path().join("base/Dockerfile"),
        "FROM busybox:latest\nRUN echo \"base v2\" > /base.txt\n",
    )
    .expect("edit dockerfile");
    let third = execute_target(dir.path(), &graph, "base:image", &opts, &reg)
        .await
        .expect("third base");
    assert!(!third.actions[0].cached, "dockerfile edit invalidates the cache");
}
```

- [ ] **Step 5: Run the tests**

Run: `./dev cargo test -p tddy-build-docker --test example_images`
Expected: PASS. The build/cache test runs real `docker build` when the daemon is up; otherwise it prints `SKIP`.

- [ ] **Step 6: Commit**

```bash
git add packages/tddy-build-docker/examples packages/tddy-build-docker/tests/example_images.rs \
        packages/tddy-build-docker/Cargo.toml
git commit -m "test(tddy-build-docker): real image set example + deps/cache verification (daemon-gated)"
```

---

## Task 11: Docs changeset

**Files:**
- Create: `docs/dev/1-WIP/tddy-build-examples.md` (changeset — do NOT edit `packages/*/docs/` directly per AGENTS.md)

- [ ] **Step 1: Write the changeset**

```markdown
# tddy-build example projects, logging & plugin inputs/outputs

## tddy-build
- Engine now logs at discovery / lowering / cycle-detection / cache / execution
  seams (`log` crate). Cycle detection emits a `warn!` naming the offending ids.
- New `io` helper (`OutputSpec`, `srcs_to_inputs`, `outputs_to_decls`) lets recipe
  plugins declare cacheable inputs/outputs in open config.
- Added a runnable `script`/`tool`/`group` example under `examples/pipeline/`.

## tddy-build-rust / -typescript / -docker
- Recipe plugins now emit `inputs`/`outputs` on their lowered actions
  (rust: `srcs`+`outputs`+`working_dir`; typescript: `srcs`+`output_dirs`;
  docker: `srcs`+`outputs` with `--iidfile`), so the content-addressed cache
  invalidates on source edits.
- Each plugin ships a real, interdependent multi-package example project
  (`examples/workspace`, `examples/monorepo`, `examples/images`) with integration
  tests covering deps-first ordering, real builds (cargo/bun/docker, tool-gated),
  cache hit/miss, and circular-reference detection.

## Note for architecture.md
Update `packages/tddy-build/docs/architecture.md` "Pipeline" + "Consumers"
sections to mention engine logging and the plugin-declared inputs/outputs once this
changeset lands (handled via the normal changeset → docs merge).
```

- [ ] **Step 2: Run the whole build-package suite one final time**

Run: `./dev cargo test -p tddy-build -p tddy-build-rust -p tddy-build-typescript -p tddy-build-docker`
Expected: PASS across all four crates.

- [ ] **Step 3: Lint and format**

Run: `./dev cargo clippy -p tddy-build -p tddy-build-rust -p tddy-build-typescript -p tddy-build-docker -- -D warnings`
Run: `./dev cargo fmt`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add docs/dev/1-WIP/tddy-build-examples.md
git commit -m "docs(tddy-build): changeset for examples, logging, and plugin inputs/outputs"
```

---

## Self-Review notes (for the implementer)

- **Cargo example isolation:** the example workspace declares its own `[workspace]`
  AND is listed in the root `Cargo.toml` `exclude`. If `cargo build -p
  tddy-build-rust` ever complains the example is part of the workspace, verify both
  are present.
- **rlib artifact names:** library outputs are `target/debug/lib<pkg>.rlib`
  (`libmathcore.rlib`, `libmathutil.rlib`). If a future cargo changes this, the
  cache "output exists" check would force a miss — update the declared `outputs`.
- **Dependent rebuilds:** the cache invalidates the *edited* target's action, not
  its dependents (no cross-target output→input edges declared here). Tests assert
  only the edited target's `cached` flag — do not over-assert dependent rebuilds.
- **Docker base tag:** `api`/`worker` Dockerfiles use `FROM example-base`, which
  exists only after `base:image` builds; `build_order` guarantees that ordering.
```
