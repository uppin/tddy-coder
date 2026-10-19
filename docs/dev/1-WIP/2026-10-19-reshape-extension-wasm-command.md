# Changeset: an opted-in extension's WASM says how to start its language server

**Date**: 2026-10-19
**Status**: 🚧 In Progress
**Type**: New feature (minimal Zed extension WASM host)
**Stack**: `#reshape` 24/24, branch `feature/reshape/extension-wasm-command`, base `feature/reshape/extension-code-actions`, green wave 4.

## Initial Discovery

[2026-10-19-reshape-extension-wasm-command-initial-discovery.md](2026-10-19-reshape-extension-wasm-command-initial-discovery.md)

## Affected Packages

- `packages/tddy-zed-extensions`: `wasm/` (host, bindings), `index.json` `wasm_enabled`, CLI `enable-wasm` /
  `disable-wasm`, `ExtensionSupport::launch` consulting the host.
- Vendored WIT `since_v0.6.0` from Zed (`crates/extension_api/wit/since_v0.6.0`, Apache-2.0) with its licence.

## Related Feature Documentation

- PRD: [PRD-2026-10-19-reshape-extension-wasm-command.md](../../ft/coder/1-WIP/PRD-2026-10-19-reshape-extension-wasm-command.md)

## Summary

Opt-in per extension; a configured `binary.path` still wins; otherwise `language_server_command`,
`language_server_initialization_options` and `language_server_workspace_configuration` from `extension.wasm` over a
minimal host that refuses every download/install call.

## Responsibility

| Symbol | Where |
|---|---|
| `WasmHost` (wasmtime `Engine`/`Linker`, `bindgen!` over WIT 0.6.x) | `src/wasm/host.rs` |
| host impls: worktree `id`, `root_path`, `read_text_file`, `which`, `shell_env`; `settings`; `platform`; `env` | `src/wasm/imports.rs` |
| refusing imports: `download_file`, `latest_github_release`, `github_release_by_tag_name`, `npm_*`, `node_binary_path`, `make_file_executable`, `set_language_server_installation_status` | `src/wasm/imports.rs` |
| `IndexEntry::wasm_enabled`, `ExtensionsCommand::{EnableWasm, DisableWasm}` | `src/index.rs`, `src/cli.rs` |

### The rules (the contract)

- **R1 — opt-in.** No `.wasm` is loaded for an extension whose `wasm_enabled` is false (the default).
- **R2 — precedence.** `lsp.<id>.binary.path` (K22) → WASM `language_server_command` → K22's refusal.
- **R3 — version gate.** `lib.version` outside 0.6.0..<0.7.0 → refused with the supported range (PRD F1 a).
- **R4 — refused imports** return `Err("tddy does not download language servers; install <server> and set lsp.<server>.binary.path")`;
  a command that failed through one carries that message into the run's refusal.
- **R5 — limits.** Each export call has a 10 s epoch deadline; exceeding it is reported and refuses the launch.
- **R6 — one wasmtime.** The host uses the wasmtime version tree-sitter's `wasm` feature pins (K20).

## Boundaries

- No downloads, GitHub, npm, process spawning from WASM; no debugger/context-server/slash-command exports.

## Dependencies

- `feature/reshape/extension-language-server` (K22) — **real edge**: settings, `ExtensionSupport::launch`.
- `feature/reshape/extension-code-actions` (K23) — line position.

## Draft PR contract

- Surface: the symbols above with `todo!()` bodies (`TODO(reshape-extension-wasm-command)`), WIT vendored, CLI wired.
- Fixture: `tests/fixtures/wasm-ext/` — a tiny code extension built once (`wasm32-wasip2`, `zed_extension_api` 0.6.x)
  from its committed source, plus the `.wasm` (PRD F2 a); exports a command from `which("fake-ls")`, a variant calling
  `download_file`, a variant that loops.
- Failing tests (`tests/wasm_command_acceptance.rs`): `an_opted_in_extension_starts_the_server_its_wasm_names`,
  `an_extension_not_opted_in_never_loads_its_wasm`, `a_configured_binary_path_wins_over_the_wasm`,
  `a_download_call_refuses_the_run_with_the_install_hint`, `an_unsupported_api_version_is_refused_with_the_range`,
  `a_looping_call_is_stopped_after_the_deadline`, `enable_and_disable_wasm_flip_the_index_entry`.

## Green wave

**Wave:** 4 of 4.
**Greenable independently:** no — needs K22 green.
**Concurrent with:** `extension-code-actions` (K23).
**Blocks:** none.

## Successor PRs

None (top of the stack).

## Scope

- [ ] USER REVIEW (ticked at `/green` start)
- [ ] Host, bindings, imports (R1–R6)
- [ ] CLI opt-in
- [ ] Fixture extension (source + wasm)
- [ ] Docs: `## Extensions with code (opt-in)` in zed-extensions.md
- [ ] Code Quality

## Testing plan

Library level with the fixture component and the fake LSP.

## Refactoring Needed

_None yet._

## Validation Results

_Not run._
