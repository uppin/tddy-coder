# tddy-build package BUILD.yaml configs

**Date:** 2026-06-20  
**Status:** Approved  
**Branch:** add-tddy-build-to-packages

## Goal

Decorate every package in the workspace with a `BUILD.yaml` so the project is buildable via `tddy-tools build` in parallel to the existing Cargo workflow. Adds a root-level `all:build` group target as a single entry point.

## Approach

**C: Per-package BUILD.yaml + root group.** Each `packages/<pkg>/BUILD.yaml` describes its own targets; a root `BUILD.yaml` adds an `all:build` group over the main deliverables. The discovery engine (`**/BUILD.yaml` glob) picks them all up automatically.

## File layout

One `BUILD.yaml` per package directory plus one at repo root:

```
BUILD.yaml                               ← root: all:build group
packages/tddy-build/BUILD.yaml
packages/tddy-build-rust/BUILD.yaml
packages/tddy-build-typescript/BUILD.yaml
packages/tddy-build-docker/BUILD.yaml
packages/tddy-core/BUILD.yaml
packages/tddy-workflow/BUILD.yaml
packages/tddy-workflow-recipes/BUILD.yaml
packages/tddy-rpc/BUILD.yaml
packages/tddy-codegen/BUILD.yaml
packages/tddy-tui/BUILD.yaml
packages/tddy-tui-testkit/BUILD.yaml
packages/tddy-service/BUILD.yaml
packages/tddy-acp-stub/BUILD.yaml
packages/tddy-connectrpc/BUILD.yaml
packages/tddy-github/BUILD.yaml
packages/tddy-livekit/BUILD.yaml
packages/tddy-livekit-screen-capture/BUILD.yaml
packages/tddy-livekit-testkit/BUILD.yaml
packages/tddy-demo/BUILD.yaml
packages/tddy-coder/BUILD.yaml
packages/tddy-tools/BUILD.yaml
packages/tddy-daemon/BUILD.yaml
packages/tddy-e2e/BUILD.yaml
packages/tddy-integration-tests/BUILD.yaml
packages/tddy-web/BUILD.yaml             ← typescript type
```

## Target ID convention

| Role | Pattern | Example |
|------|---------|---------|
| Rust library | `<pkg>:lib` | `tddy-core:lib` |
| Rust binary | `<pkg>:bin` | `tddy-coder:bin` |
| TypeScript build | `<pkg>:build` | `tddy-web:build` |
| Root group | `all:build` | `all:build` |

## Content templates

### rust_library

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

No `outputs` declared — rlib path in a cargo workspace contains a hash and is unstable.  
No `profile` in config — this is a deferred engine enhancement (CLI flag `--profile`).

### rust_binary

```yaml
schema_version: 1
targets:
  - id: "tddy-coder:bin"
    name: "tddy-coder"
    deps: ["tddy-core:lib", "tddy-build:lib", ...]
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

Binary output path (`target/debug/<name>`) is stable — declared for cache verification.

### typescript (tddy-web)

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
        - "package.json"
      output_dirs: [dist]
```

### Root group

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

Lists only leaf deliverables — transitive library deps are resolved via `deps` chains.

## Deferred

- `--profile <debug|release>` CLI flag: requires adding `profile` to `ExecuteOptions` + `LowerContext` and removing it from `RustPlugin` config structs. Tracked separately.
