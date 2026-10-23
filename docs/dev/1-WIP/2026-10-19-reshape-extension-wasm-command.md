# Changeset: an opted-in extension's WASM says how to start its language server

**Date**: 2026-10-19
**Status**: 🚧 In Progress — implemented; verification and docs pending
**Type**: New feature (minimal Zed extension WASM host)
**Stack**: `#reshape` 24/24, branch `feature/reshape/extension-wasm-command`, base `feature/reshape/extension-code-actions`, green wave 4.
**PR**: [#683](https://github.com/uppin/tddy-coder/pull/683) (draft)

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

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `extension-language-server` (K22, #681) — **real edge** | `ServerChoice::{Undeclared, Unconfigured, Configured}`, `ServerChoice::for_language`, `NoServerCommand`, `client_capabilities()`, `ExtensionSettings` | `choose_server` takes K22's choice and replaces only an `Unconfigured` one; the WASM launch carries `client_capabilities()`; `get-settings` answers from `ExtensionSettings` | change `ServerChoice`, its precedence, or the refusal text |
| `extension-install` (K19, #678) | `Index`/`IndexEntry`, `Index::{load,save}`, `ExtensionsArgs`/`ExtensionsCommand`, `cli::run`, `LibEntry { kind, version }` | adds the field `IndexEntry::wasm_enabled` and the variants `EnableWasm`/`DisableWasm` (owned here, per Responsibility); `set_wasm_enabled` goes through `Index::load`/`save` | implement `Index::{load,save}` or `cli::run`'s other arms |
| `extension-outline` (#679) | `GrammarStore`, `load_installed`, `ExtensionSupport` | nothing at contract stage; green wires `choose_server` into `load_installed` | share the grammar store's engine (see host.rs: epoch interruption would trap tree-sitter's store) |
| `extension-code-actions` (K23) | line position only | — | — |

**Sequencing of the tests:** `a_new_host…`, every acceptance test but the last, and the two `wasm::tests` reds fail on
this node's own `todo!` first. `an_opted_in_extension_starts_the_server_its_wasm_names` then needs K22's
`client_capabilities()` (green) to build its expectation; `enable_and_disable_wasm_flip_the_index_entry` needs K19's
`Index::{load,save}` (green). Both are lower waves, so no new edge.

## Draft PR contract

- Surface: the symbols above with `todo!()` bodies (`TODO(reshape-extension-wasm-command)`), WIT vendored, CLI wired.
- Fixture: `tests/fixtures/wasm-ext/` — a tiny code extension built once (`wasm32-wasip2`, `zed_extension_api` 0.6.x)
  from its committed source, plus the `.wasm` (PRD F2 a); exports a command from `which("fake-ls")`, a variant calling
  `download_file`, a variant that loops.
- Failing tests (`tests/wasm_command_acceptance.rs`): `an_opted_in_extension_starts_the_server_its_wasm_names`,
  `an_extension_not_opted_in_never_loads_its_wasm`, `a_configured_binary_path_wins_over_the_wasm`,
  `a_download_call_refuses_the_run_with_the_install_hint`, `an_unsupported_api_version_is_refused_with_the_range`,
  `a_looping_call_is_stopped_after_the_deadline`, `enable_and_disable_wasm_flip_the_index_entry`.

### As published (wave 2, commit 2)

| Symbol | Where |
|---|---|
| `EXTENSION_WASM`, `SUPPORTED_API_VERSIONS`, `CALL_DEADLINE`, `supports_api_version`, `download_refusal` | `src/wasm/mod.rs:32-55` |
| `WasmWorktree { id, root, shell_env }` | `src/wasm/mod.rs:60` |
| `WasmCommandError::{UnsupportedApiVersion, Load, Refused, DeadlineExceeded, Trapped}` | `src/wasm/mod.rs:72` |
| `choose_server(manifest, language, choice, wasm, worktree)` — R2 | `src/wasm/mod.rs:126` |
| `bindings` (`bindgen!` over `wit/since_v0.6.0`, `imports: { default: trappable }`, `worktree` → `WasmWorktree`) | `src/wasm/host.rs:24` |
| `WasmHost::{new, with_deadline, deadline, load, load_opted_in}` | `src/wasm/host.rs:36-96` |
| `WasmCommand`, `WasmExtension::{extension, language_server_command, language_server_initialization_options, language_server_workspace_configuration}` | `src/wasm/host.rs:101-170` |
| `HostState` + every generated `Host*` trait (the full 0.6.0 import set; `add_to_linker` type-checks against it) | `src/wasm/imports.rs` |
| `IndexEntry::wasm_enabled` (`#[serde(default)]`), `set_wasm_enabled` | `src/index.rs:30,58` |
| `ExtensionsCommand::{EnableWasm, DisableWasm}(WasmArgs)` | `src/cli.rs:24-50` |

- **WIT** vendored unmodified from zed `c4a82ab1ab958aa43b0c0b15c0bad4fb77a7ca7f` with `LICENSE-APACHE` (`wit/README.md`).
- **wasmtime** named directly at 36.0.19 only to turn on `component-model` (tree-sitter's `wasmtime-c-api` leaves it
  off); `cargo tree -i wasmtime` shows one version. New lock entries: wasmtime's own `component-macro`,
  `component-util`, `wit-bindgen`, plus `wit-parser` 0.236.1 and `id-arena` 2.3.0.
- **Fixture** `tests/fixtures/wasm-ext/`: one component (154,907 bytes, sha256 `4971530753ae…26720`), the three
  variants chosen by server id (`fake-ls`, `downloading-ls`, `looping-ls`); build recipe in its README.
- **The deadline** is a `with_deadline` builder so the loop test runs in 300 ms; `CALL_DEADLINE` (10 s) is the default.

### Failing tests

| Test | File | Fails on |
|---|---|---|
| `an_opted_in_extension_starts_the_server_its_wasm_names` | `tests/wasm_command_acceptance.rs:58` | `todo!` `WasmHost::new` |
| `an_extension_not_opted_in_never_loads_its_wasm` | `:98` | same |
| `a_configured_binary_path_wins_over_the_wasm` | `:121` | same |
| `a_download_call_refuses_the_run_with_the_install_hint` | `:142` | same |
| `an_unsupported_api_version_is_refused_with_the_range` | `:168` | same |
| `a_looping_call_is_stopped_after_the_deadline` | `:193` | same |
| `enable_and_disable_wasm_flip_the_index_entry` | `:228` | `todo!` `set_wasm_enabled` |
| `only_zed_extension_api_0_6_is_supported` | `src/wasm/mod.rs:144` | `todo!` `supports_api_version` |
| `the_download_refusal_names_the_server_and_its_setting` | `src/wasm/mod.rs:159` | `todo!` `download_refusal` |
| `a_new_host_stops_a_call_after_ten_seconds` | `src/wasm/host.rs:177` | `todo!` `WasmHost::new` |

Green on purpose (they pin wording and shapes the reds rely on): `the_unsupported_version_refusal_gives_the_range`
(thiserror text), `an_index_written_before_wasm_enabled_existed_reads_as_not_opted_in` (`serde(default)`),
`enable_wasm_and_disable_wasm_take_the_extension_id` (clap).

### Open for the developer (raised at contract time)

1. **The run's refusal (R3/R4/R5 "the run refuses with it")** cannot be carried by K22's `ServerChoice`: its only
   refusal is `Unconfigured(NoServerCommand)`, whose text is fixed. Carrying a `WasmCommandError` to
   `ServerBackend::without_command` needs a K22 change (e.g. a `ServerChoice` refusal variant holding text) or a
   field on `ExtensionSupport`. Not done here; the tests pin the refusal at `choose_server`.
2. **WASI.** A `wasm32-wasip2` extension imports `wasi:cli/environment`, `exit`, `stdin/stdout/stderr`, `terminal-*`,
   `clocks/wall-clock`, `filesystem/{preopens,types}`, `io/{error,poll,streams}` (0.2.0 and 0.2.6), and
   `zed_extension_api`'s `init-extension` reads `PWD` and calls `set_current_dir`. Green needs `wasmtime-wasi` 36
   (**a new dependency — not consented**) or a hand-written subset of those interfaces.
3. **The worktree at load time.** `language-server-command` takes a worktree, but `load_installed(dir)` has no
   workspace root and `LanguageSupport::launch()` takes none. Green must pick the root and the `shell_env` (process
   env?) it passes; the contract takes a `WasmWorktree` explicitly.

### As implemented (green)

- **WASI (open item 2): hand-written subset, no new crate.** `wit/wasi-0.2.6-subset/` is WIT for exactly the
  `wasi:{io,cli,clocks,filesystem}@0.2.6` functions and types the fixture component imports (read off its import types
  with `Component::component_type()`), bound by a second `bindgen!` in `src/wasm/wasi.rs` and added to the same
  `Linker`. Environment is `PWD=/` only; one preopened directory `/` that is empty and read-only (`set_current_dir`
  needs it: with no preopen `init-extension` panics with ENOTSUP); stdout/stderr accept and discard; stdin is empty; no
  terminals; `exit` traps. Replace with `wasmtime-wasi` if that dependency is ever consented.
- **Deadline (R5):** the host owns one engine with epoch interruption and a 10 ms ticker thread (stopped when the host
  and every extension loaded through it are dropped); each call sets `ceil(deadline / 10 ms) + 1` ticks.
- **The worktree (open item 3):** `load_installed` asks every extension about the process's working directory
  (`id` 1, `root` = `current_dir()`, `shell_env` = this process's environment — the CLI runs from the user's shell).
  `which` searches that env's `PATH`; `read-text-file` stays inside the root once symlinks are resolved.
- **The run's refusal (open item 1) needed no K22 change:** K22 already ships `ServerChoice::Refused(ServerRefusal)`.
  `wasm::served_by_wasm` (called from `load_installed`) turns a `WasmCommandError` into that variant with
  `error.to_string()` as the reason; only an `Unconfigured` server of an opted-in extension with a `[lib]` is ever
  handed to the WASM, and no engine starts unless one is.
- **Re-install keeps the opt-in:** `install::commit` carries `wasm_enabled` over from the previous entry (R1's default
  `false` applies to a fresh entry). Note the trade-off: a new version's code runs with the old version's opt-in.
- `get-settings` answers category `language_servers` with a server id key in the shape of Zed's `LspSettings`; any other
  category is an `Err`.

## Green wave

**Wave:** 4 of 4.
**Greenable independently:** no — needs K22 green.
**Concurrent with:** `extension-code-actions` (K23).
**Blocks:** none.

## Successor PRs

None (top of the stack).

## Scope

- [x] USER REVIEW (ticked at `/green` start)
- [x] Host, bindings, imports (R1–R6)
- [x] CLI opt-in
- [x] Fixture extension (source + wasm)
- [ ] Docs: `## Extensions with code (opt-in)` in zed-extensions.md
- [ ] Code Quality

## Testing plan

Library level with the fixture component and the fake LSP.

- [x] Create failing acceptance tests (wave 2)
- [x] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests (batched)
- [x] TDD Red — write failing unit/integration tests

## Refactoring Needed

_None yet._

## Validation Results

_Not run._
