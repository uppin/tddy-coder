# An opted-in extension's WASM says how to start its language server - PRD

**Date**: 2026-10-19
**PRD Type**: New feature (minimal Zed extension WASM host)
**Stack**: `#reshape` 24/24 (`feature/reshape/extension-wasm-command`, base `feature/reshape/extension-code-actions`;
real parent `feature/reshape/extension-language-server` — wave 4)

## Affected Features

- `docs/ft/coder/zed-extensions.md`: new section `## Extensions with code (opt-in)`.

## Summary

- By default tddy runs **no** extension code (developer decision 2026-10-19). An extension can be **opted in**:
  `tddy-tools extensions enable-wasm <id>` (recorded in `index.json`; `disable-wasm` reverses it).
- For an opted-in extension with `extension.wasm`, when one of its languages needs its server and no
  `lsp.<server>.binary.path` is set (K22 — a setting still wins, as in Zed), tddy loads the component and calls:
  - `language_server_command(server_id, worktree)` → the `LaunchSpec` program, args, env;
  - `language_server_initialization_options` and `language_server_workspace_configuration` → the spec's options and
    settings.
- **Minimal host.** Implemented: `worktree.{id, root_path, read_text_file, which, shell_env}`, `settings`
  (`language_server_settings` from K22's file), `platform`, `env`. Everything that downloads or installs —
  `download_file`, `latest_github_release`, `github_release_by_tag_name`, `npm_*`, `node_binary_path`,
  `make_file_executable`, `set_language_server_installation_status` — returns an error to the extension:
  `tddy does not download language servers; install <server> and set lsp.<server>.binary.path`, and the run's refusal
  carries that message.

## Background

- Zed's WIT `zed:extension` has ten versions (`since_v0.0.1` … `since_v0.8.0`); stable Zed accepts up to 0.6.x. Each
  version is a separate binding.
- tree-sitter's `wasm` feature (K20) already brings wasmtime; the component host must use that same version.

## Proposed Changes

### What's Changing

- `tddy-zed-extensions::wasm`: wasmtime component-model host (`bindgen!` over vendored WIT files from Zed,
  Apache-2.0, with their licence), one `Engine` shared with the grammar store where the versions allow it.
- Version gate: an extension whose `lib.version` is outside the supported range is refused with the range (F1).
- Resource limits: fuel/epoch timeout per call (a call that does not return within 10 s is stopped and reported).

### What's Staying the Same

- Extensions not opted in: no WASM loaded. A configured binary path is used before any WASM.

## Acceptance Criteria

- [ ] A fixture code extension (built for the supported WIT version, vendored) that returns a command from
      `which("fake-ls")` starts that server when opted in, and is never loaded when not.
- [ ] An extension calling `download_file` fails its command with the message above, and the run refuses with it.
- [ ] A configured `binary.path` wins over the WASM.
- [ ] An extension built for an unsupported API version is refused with the supported range.
- [ ] A call that loops is stopped after the timeout.

## Open questions

- **F1 — supported WIT versions.** (a) 0.6.x only, what stable Zed builds against today (recommended: one binding);
  (b) 0.1.0–0.6.x (most registry extensions, five bindings); (c) all ten.
- **F2 — fixture.** (a) A tiny code extension built once from Rust source in `tests/fixtures/` with the
  `wasm32-wasip2` target and committed as `.wasm` with its source (recommended); (b) built by a build script.
- **F3 — opt-in surface.** (a) `extensions enable-wasm <id>` (recommended); (b) a setting in `settings.json`.

## References

- Zed `crates/extension_host/src/wasm_host/wit.rs` (version range), `crates/extension_api/wit/since_v0.6.0`.
