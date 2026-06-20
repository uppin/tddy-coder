# tddy-build Package BUILD.yaml Configs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `BUILD.yaml` to every package in the workspace so the project is buildable via `tddy-tools build` in parallel to the existing Cargo workflow, with a root `all:build` group target as a single entry point.

**Architecture:** Per-package BUILD.yaml files co-located in each `packages/<pkg>/` directory, using `rust_library`/`rust_binary`/`typescript` target types from the existing plugin set. A root `BUILD.yaml` adds an `all:build` group over the main deliverables (tddy-coder, tddy-tools, tddy-daemon, tddy-web). The `deps` field in each file mirrors the direct workspace Cargo dependencies, giving the build engine the right execution order.

**Tech Stack:** Rust (cargo workspace), Bun (TypeScript), tddy-build engine (`rust_library`, `rust_binary`, `typescript`, `group` plugin types), `tddy-tools build-list` / `build` CLI for verification.

## Global Constraints

- `schema_version: 1` in every BUILD.yaml
- Target IDs: `<pkg>:lib` for libraries, `<pkg>:bin` for binaries, `<pkg>:build` for TypeScript
- `srcs` globs are repo-root-relative (e.g. `packages/tddy-core/src/**/*.rs`)
- No `profile` field in any config — it is not yet wired as a CLI arg (deferred engine enhancement)
- `outputs` declared only for binaries (stable path `target/debug/<name>`) — rlib paths are unstable in cargo workspaces
- `deps` lists direct workspace dependencies only (not transitive)
- Dev-only workspace deps (e.g. test kits in `[dev-dependencies]`) are NOT listed in BUILD `deps`
- All commands run via `./dev <cmd>` to use the nix dev shell

---

## File Map

**Files to create (26 total):**

```
BUILD.yaml                                         root group
packages/tddy-build/BUILD.yaml
packages/tddy-build-rust/BUILD.yaml
packages/tddy-build-typescript/BUILD.yaml
packages/tddy-build-docker/BUILD.yaml
packages/tddy-workflow/BUILD.yaml
packages/tddy-rpc/BUILD.yaml
packages/tddy-codegen/BUILD.yaml
packages/tddy-livekit-testkit/BUILD.yaml
packages/tddy-acp-stub/BUILD.yaml
packages/tddy-core/BUILD.yaml
packages/tddy-connectrpc/BUILD.yaml
packages/tddy-tui/BUILD.yaml
packages/tddy-workflow-recipes/BUILD.yaml
packages/tddy-service/BUILD.yaml
packages/tddy-github/BUILD.yaml
packages/tddy-livekit/BUILD.yaml
packages/tddy-tui-testkit/BUILD.yaml
packages/tddy-livekit-screen-capture/BUILD.yaml
packages/tddy-e2e/BUILD.yaml
packages/tddy-coder/BUILD.yaml
packages/tddy-tools/BUILD.yaml
packages/tddy-daemon/BUILD.yaml
packages/tddy-demo/BUILD.yaml
packages/tddy-integration-tests/BUILD.yaml
packages/tddy-web/BUILD.yaml
```

**No existing files modified** — this is pure config file creation.

---

## Verification helper

Used at the end of each task to confirm new YAML files parse correctly:

```bash
python3 -c "
import yaml, sys
files = sys.argv[1:]
errors = []
for f in files:
    try:
        data = yaml.safe_load(open(f))
        assert data.get('schema_version') == 1, 'missing schema_version: 1'
        assert data.get('targets'), 'no targets'
        for t in data['targets']:
            assert t.get('id'), f'target missing id'
    except Exception as e:
        errors.append(f'{f}: {e}')
if errors:
    [print(e, file=sys.stderr) for e in errors]; sys.exit(1)
print(f'OK: {len(files)} file(s) valid')
"
```

Save as `scripts/check-build-yaml.py` in Task 1.

---

## Task 1: Standalone leaf packages (no workspace deps)

Packages: `tddy-build`, `tddy-workflow`, `tddy-rpc`, `tddy-codegen`, `tddy-livekit-testkit` (libs), `tddy-acp-stub` (binary).

**Files:**
- Create: `scripts/check-build-yaml.py`
- Create: `packages/tddy-build/BUILD.yaml`
- Create: `packages/tddy-workflow/BUILD.yaml`
- Create: `packages/tddy-rpc/BUILD.yaml`
- Create: `packages/tddy-codegen/BUILD.yaml`
- Create: `packages/tddy-livekit-testkit/BUILD.yaml`
- Create: `packages/tddy-acp-stub/BUILD.yaml`

**Interfaces:**
- Produces: `tddy-build:lib`, `tddy-workflow:lib`, `tddy-rpc:lib`, `tddy-codegen:lib`, `tddy-livekit-testkit:lib`, `tddy-acp-stub:bin`

- [ ] **Step 1: Create the verification helper**

```python
# scripts/check-build-yaml.py
import yaml, sys
files = sys.argv[1:]
errors = []
for f in files:
    try:
        data = yaml.safe_load(open(f))
        assert data.get('schema_version') == 1, 'missing schema_version: 1'
        assert data.get('targets'), 'no targets'
        for t in data['targets']:
            assert t.get('id'), 'target missing id'
    except Exception as e:
        errors.append(f'{f}: {e}')
if errors:
    [print(e, file=sys.stderr) for e in errors]
    sys.exit(1)
print(f'OK: {len(files)} file(s) valid')
```

- [ ] **Step 2: Write packages/tddy-build/BUILD.yaml**

```yaml
schema_version: 1
targets:
  - id: "tddy-build:lib"
    name: "tddy-build"
    config:
      type: rust_library
      package: tddy-build
      srcs:
        - "packages/tddy-build/src/**/*.rs"
        - "packages/tddy-build/Cargo.toml"
```

- [ ] **Step 3: Write packages/tddy-workflow/BUILD.yaml**

```yaml
schema_version: 1
targets:
  - id: "tddy-workflow:lib"
    name: "tddy-workflow"
    config:
      type: rust_library
      package: tddy-workflow
      srcs:
        - "packages/tddy-workflow/src/**/*.rs"
        - "packages/tddy-workflow/Cargo.toml"
```

- [ ] **Step 4: Write packages/tddy-rpc/BUILD.yaml**

```yaml
schema_version: 1
targets:
  - id: "tddy-rpc:lib"
    name: "tddy-rpc"
    config:
      type: rust_library
      package: tddy-rpc
      srcs:
        - "packages/tddy-rpc/src/**/*.rs"
        - "packages/tddy-rpc/Cargo.toml"
```

- [ ] **Step 5: Write packages/tddy-codegen/BUILD.yaml**

```yaml
schema_version: 1
targets:
  - id: "tddy-codegen:lib"
    name: "tddy-codegen"
    config:
      type: rust_library
      package: tddy-codegen
      srcs:
        - "packages/tddy-codegen/src/**/*.rs"
        - "packages/tddy-codegen/Cargo.toml"
```

- [ ] **Step 6: Write packages/tddy-livekit-testkit/BUILD.yaml**

```yaml
schema_version: 1
targets:
  - id: "tddy-livekit-testkit:lib"
    name: "tddy-livekit-testkit"
    config:
      type: rust_library
      package: tddy-livekit-testkit
      srcs:
        - "packages/tddy-livekit-testkit/src/**/*.rs"
        - "packages/tddy-livekit-testkit/Cargo.toml"
```

- [ ] **Step 7: Write packages/tddy-acp-stub/BUILD.yaml**

```yaml
schema_version: 1
targets:
  - id: "tddy-acp-stub:bin"
    name: "tddy-acp-stub"
    config:
      type: rust_binary
      package: tddy-acp-stub
      bin_name: tddy-acp-stub
      srcs:
        - "packages/tddy-acp-stub/src/**/*.rs"
        - "packages/tddy-acp-stub/Cargo.toml"
      outputs:
        - path: "target/debug/tddy-acp-stub"
          kind: file
```

- [ ] **Step 8: Verify**

```bash
python3 scripts/check-build-yaml.py \
  packages/tddy-build/BUILD.yaml \
  packages/tddy-workflow/BUILD.yaml \
  packages/tddy-rpc/BUILD.yaml \
  packages/tddy-codegen/BUILD.yaml \
  packages/tddy-livekit-testkit/BUILD.yaml \
  packages/tddy-acp-stub/BUILD.yaml
```

Expected: `OK: 6 file(s) valid`

- [ ] **Step 9: Commit**

```bash
git add scripts/check-build-yaml.py \
  packages/tddy-build/BUILD.yaml \
  packages/tddy-workflow/BUILD.yaml \
  packages/tddy-rpc/BUILD.yaml \
  packages/tddy-codegen/BUILD.yaml \
  packages/tddy-livekit-testkit/BUILD.yaml \
  packages/tddy-acp-stub/BUILD.yaml
git commit -m "build: BUILD.yaml for standalone leaf packages (layer 0)"
```

---

## Task 2: Build plugins + tddy-core + tddy-connectrpc (Layer 1)

Packages: `tddy-build-rust`, `tddy-build-typescript`, `tddy-build-docker`, `tddy-core`, `tddy-connectrpc`.

**Files:**
- Create: `packages/tddy-build-rust/BUILD.yaml`
- Create: `packages/tddy-build-typescript/BUILD.yaml`
- Create: `packages/tddy-build-docker/BUILD.yaml`
- Create: `packages/tddy-core/BUILD.yaml`
- Create: `packages/tddy-connectrpc/BUILD.yaml`

**Interfaces:**
- Consumes: `tddy-build:lib`, `tddy-workflow:lib`, `tddy-rpc:lib` (from Task 1)
- Produces: `tddy-build-rust:lib`, `tddy-build-typescript:lib`, `tddy-build-docker:lib`, `tddy-core:lib`, `tddy-connectrpc:lib`

- [ ] **Step 1: Write packages/tddy-build-rust/BUILD.yaml**

```yaml
schema_version: 1
targets:
  - id: "tddy-build-rust:lib"
    name: "tddy-build-rust"
    deps: ["tddy-build:lib"]
    config:
      type: rust_library
      package: tddy-build-rust
      srcs:
        - "packages/tddy-build-rust/src/**/*.rs"
        - "packages/tddy-build-rust/Cargo.toml"
```

- [ ] **Step 2: Write packages/tddy-build-typescript/BUILD.yaml**

```yaml
schema_version: 1
targets:
  - id: "tddy-build-typescript:lib"
    name: "tddy-build-typescript"
    deps: ["tddy-build:lib"]
    config:
      type: rust_library
      package: tddy-build-typescript
      srcs:
        - "packages/tddy-build-typescript/src/**/*.rs"
        - "packages/tddy-build-typescript/Cargo.toml"
```

- [ ] **Step 3: Write packages/tddy-build-docker/BUILD.yaml**

```yaml
schema_version: 1
targets:
  - id: "tddy-build-docker:lib"
    name: "tddy-build-docker"
    deps: ["tddy-build:lib"]
    config:
      type: rust_library
      package: tddy-build-docker
      srcs:
        - "packages/tddy-build-docker/src/**/*.rs"
        - "packages/tddy-build-docker/Cargo.toml"
```

- [ ] **Step 4: Write packages/tddy-core/BUILD.yaml**

```yaml
schema_version: 1
targets:
  - id: "tddy-core:lib"
    name: "tddy-core"
    deps: ["tddy-workflow:lib"]
    config:
      type: rust_library
      package: tddy-core
      srcs:
        - "packages/tddy-core/src/**/*.rs"
        - "packages/tddy-core/Cargo.toml"
```

- [ ] **Step 5: Write packages/tddy-connectrpc/BUILD.yaml**

(`tddy-connectrpc` depends on `tddy-rpc` at runtime; `tddy-service` is a dev-dep only — excluded.)

```yaml
schema_version: 1
targets:
  - id: "tddy-connectrpc:lib"
    name: "tddy-connectrpc"
    deps: ["tddy-rpc:lib"]
    config:
      type: rust_library
      package: tddy-connectrpc
      srcs:
        - "packages/tddy-connectrpc/src/**/*.rs"
        - "packages/tddy-connectrpc/Cargo.toml"
```

- [ ] **Step 6: Verify**

```bash
python3 scripts/check-build-yaml.py \
  packages/tddy-build-rust/BUILD.yaml \
  packages/tddy-build-typescript/BUILD.yaml \
  packages/tddy-build-docker/BUILD.yaml \
  packages/tddy-core/BUILD.yaml \
  packages/tddy-connectrpc/BUILD.yaml
```

Expected: `OK: 5 file(s) valid`

- [ ] **Step 7: Commit**

```bash
git add \
  packages/tddy-build-rust/BUILD.yaml \
  packages/tddy-build-typescript/BUILD.yaml \
  packages/tddy-build-docker/BUILD.yaml \
  packages/tddy-core/BUILD.yaml \
  packages/tddy-connectrpc/BUILD.yaml
git commit -m "build: BUILD.yaml for build plugins + tddy-core + tddy-connectrpc (layer 1)"
```

---

## Task 3: Mid-layer libs — tddy-tui, tddy-workflow-recipes, tddy-service (Layers 2–3)

**Files:**
- Create: `packages/tddy-tui/BUILD.yaml`
- Create: `packages/tddy-workflow-recipes/BUILD.yaml`
- Create: `packages/tddy-service/BUILD.yaml`

**Interfaces:**
- Consumes: `tddy-core:lib`, `tddy-workflow:lib`, `tddy-rpc:lib` (Tasks 1–2)
- Produces: `tddy-tui:lib`, `tddy-workflow-recipes:lib`, `tddy-service:lib`

- [ ] **Step 1: Write packages/tddy-tui/BUILD.yaml**

```yaml
schema_version: 1
targets:
  - id: "tddy-tui:lib"
    name: "tddy-tui"
    deps: ["tddy-core:lib"]
    config:
      type: rust_library
      package: tddy-tui
      srcs:
        - "packages/tddy-tui/src/**/*.rs"
        - "packages/tddy-tui/Cargo.toml"
```

- [ ] **Step 2: Write packages/tddy-workflow-recipes/BUILD.yaml**

```yaml
schema_version: 1
targets:
  - id: "tddy-workflow-recipes:lib"
    name: "tddy-workflow-recipes"
    deps:
      - "tddy-core:lib"
      - "tddy-workflow:lib"
    config:
      type: rust_library
      package: tddy-workflow-recipes
      srcs:
        - "packages/tddy-workflow-recipes/src/**/*.rs"
        - "packages/tddy-workflow-recipes/Cargo.toml"
```

- [ ] **Step 3: Write packages/tddy-service/BUILD.yaml**

(`tddy-codegen` is a Cargo build-dep used at compile time by build.rs, not a runtime dep — excluded from BUILD deps.)

```yaml
schema_version: 1
targets:
  - id: "tddy-service:lib"
    name: "tddy-service"
    deps:
      - "tddy-core:lib"
      - "tddy-workflow:lib"
      - "tddy-workflow-recipes:lib"
      - "tddy-rpc:lib"
      - "tddy-tui:lib"
    config:
      type: rust_library
      package: tddy-service
      srcs:
        - "packages/tddy-service/src/**/*.rs"
        - "packages/tddy-service/Cargo.toml"
```

- [ ] **Step 4: Verify**

```bash
python3 scripts/check-build-yaml.py \
  packages/tddy-tui/BUILD.yaml \
  packages/tddy-workflow-recipes/BUILD.yaml \
  packages/tddy-service/BUILD.yaml
```

Expected: `OK: 3 file(s) valid`

- [ ] **Step 5: Commit**

```bash
git add \
  packages/tddy-tui/BUILD.yaml \
  packages/tddy-workflow-recipes/BUILD.yaml \
  packages/tddy-service/BUILD.yaml
git commit -m "build: BUILD.yaml for tddy-tui, tddy-workflow-recipes, tddy-service (layers 2-3)"
```

---

## Task 4: Integration packages — tddy-github, tddy-livekit, tddy-tui-testkit, tddy-livekit-screen-capture, tddy-e2e (Layers 4–5)

**Files:**
- Create: `packages/tddy-github/BUILD.yaml`
- Create: `packages/tddy-livekit/BUILD.yaml`
- Create: `packages/tddy-tui-testkit/BUILD.yaml`
- Create: `packages/tddy-livekit-screen-capture/BUILD.yaml`
- Create: `packages/tddy-e2e/BUILD.yaml`

**Interfaces:**
- Consumes: `tddy-rpc:lib`, `tddy-service:lib`, `tddy-livekit:lib`, `tddy-tui:lib`, `tddy-tui-testkit:lib`, `tddy-core:lib`, `tddy-workflow-recipes:lib` (Tasks 1–3)
- Produces: `tddy-github:lib`, `tddy-livekit:lib`, `tddy-tui-testkit:lib`, `tddy-livekit-screen-capture:bin`, `tddy-e2e:lib`

- [ ] **Step 1: Write packages/tddy-github/BUILD.yaml**

```yaml
schema_version: 1
targets:
  - id: "tddy-github:lib"
    name: "tddy-github"
    deps:
      - "tddy-rpc:lib"
      - "tddy-service:lib"
    config:
      type: rust_library
      package: tddy-github
      srcs:
        - "packages/tddy-github/src/**/*.rs"
        - "packages/tddy-github/Cargo.toml"
```

- [ ] **Step 2: Write packages/tddy-livekit/BUILD.yaml**

(`tddy-livekit-testkit` is a dev-dep only — excluded.)

```yaml
schema_version: 1
targets:
  - id: "tddy-livekit:lib"
    name: "tddy-livekit"
    deps:
      - "tddy-rpc:lib"
      - "tddy-service:lib"
    config:
      type: rust_library
      package: tddy-livekit
      srcs:
        - "packages/tddy-livekit/src/**/*.rs"
        - "packages/tddy-livekit/Cargo.toml"
```

- [ ] **Step 3: Write packages/tddy-tui-testkit/BUILD.yaml**

(`tddy-core` and `tddy-workflow-recipes` are dev-deps only — excluded.)

```yaml
schema_version: 1
targets:
  - id: "tddy-tui-testkit:lib"
    name: "tddy-tui-testkit"
    deps:
      - "tddy-service:lib"
    config:
      type: rust_library
      package: tddy-tui-testkit
      srcs:
        - "packages/tddy-tui-testkit/src/**/*.rs"
        - "packages/tddy-tui-testkit/Cargo.toml"
```

- [ ] **Step 4: Write packages/tddy-livekit-screen-capture/BUILD.yaml**

```yaml
schema_version: 1
targets:
  - id: "tddy-livekit-screen-capture:bin"
    name: "tddy-livekit-screen-capture"
    deps:
      - "tddy-livekit:lib"
    config:
      type: rust_binary
      package: tddy-livekit-screen-capture
      bin_name: tddy-livekit-screen-capture
      srcs:
        - "packages/tddy-livekit-screen-capture/src/**/*.rs"
        - "packages/tddy-livekit-screen-capture/Cargo.toml"
      outputs:
        - path: "target/debug/tddy-livekit-screen-capture"
          kind: file
```

- [ ] **Step 5: Write packages/tddy-e2e/BUILD.yaml**

(`tddy-livekit` and `tddy-livekit-testkit` are optional features — include `tddy-livekit:lib` for full-feature build ordering; omit `tddy-livekit-testkit` as it's a test helper.)

```yaml
schema_version: 1
targets:
  - id: "tddy-e2e:lib"
    name: "tddy-e2e"
    deps:
      - "tddy-core:lib"
      - "tddy-workflow-recipes:lib"
      - "tddy-rpc:lib"
      - "tddy-service:lib"
      - "tddy-tui:lib"
      - "tddy-tui-testkit:lib"
      - "tddy-livekit:lib"
    config:
      type: rust_library
      package: tddy-e2e
      srcs:
        - "packages/tddy-e2e/src/**/*.rs"
        - "packages/tddy-e2e/Cargo.toml"
```

- [ ] **Step 6: Verify**

```bash
python3 scripts/check-build-yaml.py \
  packages/tddy-github/BUILD.yaml \
  packages/tddy-livekit/BUILD.yaml \
  packages/tddy-tui-testkit/BUILD.yaml \
  packages/tddy-livekit-screen-capture/BUILD.yaml \
  packages/tddy-e2e/BUILD.yaml
```

Expected: `OK: 5 file(s) valid`

- [ ] **Step 7: Commit**

```bash
git add \
  packages/tddy-github/BUILD.yaml \
  packages/tddy-livekit/BUILD.yaml \
  packages/tddy-tui-testkit/BUILD.yaml \
  packages/tddy-livekit-screen-capture/BUILD.yaml \
  packages/tddy-e2e/BUILD.yaml
git commit -m "build: BUILD.yaml for integration packages (layers 4-5)"
```

---

## Task 5: Main binaries — tddy-coder and tddy-tools (Layer 6)

**Files:**
- Create: `packages/tddy-coder/BUILD.yaml`
- Create: `packages/tddy-tools/BUILD.yaml`

**Interfaces:**
- Consumes: all `:lib` targets from Tasks 1–4
- Produces: `tddy-coder:bin`, `tddy-tools:bin`

- [ ] **Step 1: Write packages/tddy-coder/BUILD.yaml**

```yaml
schema_version: 1
targets:
  - id: "tddy-coder:bin"
    name: "tddy-coder"
    deps:
      - "tddy-core:lib"
      - "tddy-build:lib"
      - "tddy-build-rust:lib"
      - "tddy-build-typescript:lib"
      - "tddy-build-docker:lib"
      - "tddy-tui:lib"
      - "tddy-workflow-recipes:lib"
      - "tddy-rpc:lib"
      - "tddy-service:lib"
      - "tddy-livekit:lib"
      - "tddy-connectrpc:lib"
      - "tddy-github:lib"
    config:
      type: rust_binary
      package: tddy-coder
      bin_name: tddy-coder
      srcs:
        - "packages/tddy-coder/src/**/*.rs"
        - "packages/tddy-coder/Cargo.toml"
      outputs:
        - path: "target/debug/tddy-coder"
          kind: file
```

- [ ] **Step 2: Write packages/tddy-tools/BUILD.yaml**

(`tddy-livekit` is an optional dep but included for full build ordering.)

```yaml
schema_version: 1
targets:
  - id: "tddy-tools:bin"
    name: "tddy-tools"
    deps:
      - "tddy-core:lib"
      - "tddy-build:lib"
      - "tddy-build-rust:lib"
      - "tddy-build-typescript:lib"
      - "tddy-build-docker:lib"
      - "tddy-workflow-recipes:lib"
      - "tddy-service:lib"
      - "tddy-livekit:lib"
    config:
      type: rust_binary
      package: tddy-tools
      bin_name: tddy-tools
      srcs:
        - "packages/tddy-tools/src/**/*.rs"
        - "packages/tddy-tools/Cargo.toml"
      outputs:
        - path: "target/debug/tddy-tools"
          kind: file
```

- [ ] **Step 3: Verify**

```bash
python3 scripts/check-build-yaml.py \
  packages/tddy-coder/BUILD.yaml \
  packages/tddy-tools/BUILD.yaml
```

Expected: `OK: 2 file(s) valid`

- [ ] **Step 4: Commit**

```bash
git add \
  packages/tddy-coder/BUILD.yaml \
  packages/tddy-tools/BUILD.yaml
git commit -m "build: BUILD.yaml for tddy-coder and tddy-tools (layer 6)"
```

---

## Task 6: Top-level packages — tddy-daemon, tddy-demo, tddy-integration-tests (Layer 7)

**Files:**
- Create: `packages/tddy-daemon/BUILD.yaml`
- Create: `packages/tddy-demo/BUILD.yaml`
- Create: `packages/tddy-integration-tests/BUILD.yaml`

**Interfaces:**
- Consumes: `tddy-coder:bin`, `tddy-core:lib`, `tddy-github:lib`, `tddy-livekit:lib`, `tddy-rpc:lib`, `tddy-service:lib`, `tddy-connectrpc:lib`, `tddy-workflow:lib`, `tddy-workflow-recipes:lib`, `tddy-daemon:bin`
- Produces: `tddy-daemon:bin`, `tddy-demo:bin`, `tddy-integration-tests:lib`

- [ ] **Step 1: Write packages/tddy-daemon/BUILD.yaml**

(`tddy-daemon` depends on the `tddy-coder` library, so `tddy-coder:bin` is the right dep — running `cargo build -p tddy-coder --bin tddy-coder` builds the lib too.)

```yaml
schema_version: 1
targets:
  - id: "tddy-daemon:bin"
    name: "tddy-daemon"
    deps:
      - "tddy-coder:bin"
      - "tddy-core:lib"
      - "tddy-github:lib"
      - "tddy-livekit:lib"
      - "tddy-rpc:lib"
      - "tddy-service:lib"
      - "tddy-connectrpc:lib"
    config:
      type: rust_binary
      package: tddy-daemon
      bin_name: tddy-daemon
      srcs:
        - "packages/tddy-daemon/src/**/*.rs"
        - "packages/tddy-daemon/Cargo.toml"
      outputs:
        - path: "target/debug/tddy-daemon"
          kind: file
```

- [ ] **Step 2: Write packages/tddy-demo/BUILD.yaml**

```yaml
schema_version: 1
targets:
  - id: "tddy-demo:bin"
    name: "tddy-demo"
    deps:
      - "tddy-coder:bin"
    config:
      type: rust_binary
      package: tddy-demo
      bin_name: tddy-demo
      srcs:
        - "packages/tddy-demo/src/**/*.rs"
        - "packages/tddy-demo/Cargo.toml"
      outputs:
        - path: "target/debug/tddy-demo"
          kind: file
```

- [ ] **Step 3: Write packages/tddy-integration-tests/BUILD.yaml**

```yaml
schema_version: 1
targets:
  - id: "tddy-integration-tests:lib"
    name: "tddy-integration-tests"
    deps:
      - "tddy-core:lib"
      - "tddy-workflow:lib"
      - "tddy-workflow-recipes:lib"
      - "tddy-daemon:bin"
    config:
      type: rust_library
      package: tddy-integration-tests
      srcs:
        - "packages/tddy-integration-tests/src/**/*.rs"
        - "packages/tddy-integration-tests/Cargo.toml"
```

- [ ] **Step 4: Verify**

```bash
python3 scripts/check-build-yaml.py \
  packages/tddy-daemon/BUILD.yaml \
  packages/tddy-demo/BUILD.yaml \
  packages/tddy-integration-tests/BUILD.yaml
```

Expected: `OK: 3 file(s) valid`

- [ ] **Step 5: Commit**

```bash
git add \
  packages/tddy-daemon/BUILD.yaml \
  packages/tddy-demo/BUILD.yaml \
  packages/tddy-integration-tests/BUILD.yaml
git commit -m "build: BUILD.yaml for tddy-daemon, tddy-demo, tddy-integration-tests (layer 7)"
```

---

## Task 7: TypeScript web package — tddy-web

**Files:**
- Create: `packages/tddy-web/BUILD.yaml`

**Interfaces:**
- Produces: `tddy-web:build`

Note: `tddy-web`'s `package.json` has a `prebuild` lifecycle script (`bun run --cwd ../tddy-livekit-web build`) that runs automatically before the main build. The `typescript` plugin runs `bun run build` which triggers it via Bun's lifecycle hooks — no separate BUILD target needed for `tddy-livekit-web`.

- [ ] **Step 1: Write packages/tddy-web/BUILD.yaml**

```yaml
schema_version: 1
targets:
  - id: "tddy-web:build"
    name: "tddy-web"
    config:
      type: typescript
      package_dir: packages/tddy-web
      build_script: build
      srcs:
        - "src/**/*.ts"
        - "src/**/*.tsx"
        - "src/**/*.css"
        - "index.html"
        - "package.json"
      output_dirs: [dist]
```

- [ ] **Step 2: Verify**

```bash
python3 scripts/check-build-yaml.py packages/tddy-web/BUILD.yaml
```

Expected: `OK: 1 file(s) valid`

- [ ] **Step 3: Commit**

```bash
git add packages/tddy-web/BUILD.yaml
git commit -m "build: BUILD.yaml for tddy-web (typescript)"
```

---

## Task 8: Root group target + end-to-end verification

**Files:**
- Create: `BUILD.yaml`

**Interfaces:**
- Consumes: `tddy-coder:bin`, `tddy-tools:bin`, `tddy-daemon:bin`, `tddy-web:build`
- Produces: `all:build` (entry point for full workspace build)

- [ ] **Step 1: Write root BUILD.yaml**

```yaml
schema_version: 1
targets:
  - id: "all:build"
    name: "Build all deliverables"
    config:
      type: group
      member_ids:
        - "tddy-coder:bin"
        - "tddy-tools:bin"
        - "tddy-daemon:bin"
        - "tddy-web:build"
```

- [ ] **Step 2: Verify root file**

```bash
python3 scripts/check-build-yaml.py BUILD.yaml
```

Expected: `OK: 1 file(s) valid`

- [ ] **Step 3: Verify all 25 BUILD.yaml files parse**

```bash
python3 scripts/check-build-yaml.py \
  BUILD.yaml \
  packages/tddy-build/BUILD.yaml \
  packages/tddy-build-rust/BUILD.yaml \
  packages/tddy-build-typescript/BUILD.yaml \
  packages/tddy-build-docker/BUILD.yaml \
  packages/tddy-workflow/BUILD.yaml \
  packages/tddy-rpc/BUILD.yaml \
  packages/tddy-codegen/BUILD.yaml \
  packages/tddy-livekit-testkit/BUILD.yaml \
  packages/tddy-acp-stub/BUILD.yaml \
  packages/tddy-core/BUILD.yaml \
  packages/tddy-connectrpc/BUILD.yaml \
  packages/tddy-tui/BUILD.yaml \
  packages/tddy-workflow-recipes/BUILD.yaml \
  packages/tddy-service/BUILD.yaml \
  packages/tddy-github/BUILD.yaml \
  packages/tddy-livekit/BUILD.yaml \
  packages/tddy-tui-testkit/BUILD.yaml \
  packages/tddy-livekit-screen-capture/BUILD.yaml \
  packages/tddy-e2e/BUILD.yaml \
  packages/tddy-coder/BUILD.yaml \
  packages/tddy-tools/BUILD.yaml \
  packages/tddy-daemon/BUILD.yaml \
  packages/tddy-demo/BUILD.yaml \
  packages/tddy-integration-tests/BUILD.yaml \
  packages/tddy-web/BUILD.yaml
```

Expected: `OK: 26 file(s) valid`

- [ ] **Step 4: Build tddy-tools and run build-list**

```bash
./dev cargo build -p tddy-tools 2>&1 | tail -5
./dev cargo run -p tddy-tools -- build-list --repo-dir . 2>/dev/null \
  | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['total'],'targets found')"
```

Expected output: `26 targets found` (or similar count — one per BUILD target across all files).

- [ ] **Step 5: Verify all:build dry-run resolves the full graph**

```bash
./dev cargo run -p tddy-tools -- build --repo-dir . --target all:build --dry-run 2>/dev/null \
  | python3 -c "
import json, sys
d = json.load(sys.stdin)
actions = d.get('actions', [])
print(f'{len(actions)} actions planned')
for a in actions:
    print(' ', a['argv'][:3])
"
```

Expected: multiple actions printed, starting with leaf libs and ending with the binaries.

- [ ] **Step 6: Commit root BUILD.yaml**

```bash
git add BUILD.yaml
git commit -m "build: root BUILD.yaml with all:build group — completes workspace coverage"
```

---

## Self-Review

**Spec coverage check:**
- ✅ Per-package BUILD.yaml for all 25 packages (Tasks 1–7)
- ✅ Root BUILD.yaml with `all:build` group (Task 8)
- ✅ Target IDs follow `<pkg>:lib` / `<pkg>:bin` / `<pkg>:build` convention
- ✅ No `profile` in any config (deferred engine enhancement)
- ✅ `outputs` only on binaries
- ✅ `deps` lists direct workspace runtime deps only (dev-deps excluded)
- ✅ TypeScript uses `typescript` plugin with `output_dirs: [dist]`
- ✅ End-to-end verification via `build-list` + `--dry-run`

**Placeholder scan:** None found. All YAML is complete and literal.

**Type consistency:** All target IDs referenced in `deps` match exactly the IDs produced in earlier tasks. `tddy-coder:bin` used as dep in Tasks 6 and 7 matches the ID defined in Task 5.
