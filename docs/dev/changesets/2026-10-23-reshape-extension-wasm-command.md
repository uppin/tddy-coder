# 2026-10-23 — an opted-in extension's WASM names its language server's command, over a minimal host (`#reshape` 24/24, #683)

**Type:** Feature

Package: `tddy-zed-extensions`. No new external dependency: `wasmtime` 36.0.19 is named directly only to turn on
`component-model` on the crate tree-sitter's `wasm` feature already brings (`cargo tree -i wasmtime` shows one
version); new lock entries are wasmtime's own `component-macro`, `component-util`, `wit-bindgen`, plus `wit-parser`
0.236.1 and `id-arena` 2.3.0. Feature docs:
[Zed extensions § Extensions with code (opt-in)](../../ft/coder/zed-extensions.md#extensions-with-code-opt-in).

## `tddy-zed-extensions`

- `wasm/mod.rs`: `EXTENSION_WASM`, `SUPPORTED_API_VERSIONS` (`0.6.0..<0.7.0`), `CALL_DEADLINE` (10 s),
  `supports_api_version`, `download_refusal`, `WasmWorktree { id, root, shell_env }`,
  `WasmCommandError::{UnsupportedApiVersion, Load, Refused, DeadlineExceeded, Trapped}`. `choose_server` (R2) keeps a
  `Configured` or `Undeclared` choice and asks the WASM only for an `Unconfigured` one, building a `LaunchSpec` with
  `client_capabilities()` and the manifest's `language_ids` entry. `served` wraps `server::served` (K22/K23), and
  `served_by_wasm` hands each language's `Served` triple through: `OptedInWasm::serve` asks only an opted-in
  extension, `WasmLoader` loads each one at most once and starts the host only for the first that needs it, and a
  `WasmCommandError` becomes `ServerChoice::Refused` with its text. The code-action kinds stay the manifest entry's
  for the server — the WASM names only its command. The worktree is the process's working directory, id 1, with the
  process environment as `shell-env`.
- `wasm/host.rs`: `bindings` (`bindgen!` over `wit/since_v0.6.0`, `imports: { default: trappable }`, `worktree` →
  `WasmWorktree`); `WasmHost::{new, with_deadline, deadline, load, load_opted_in}` — its own `Engine` with epoch
  interruption (one *version* with tree-sitter, not one engine: an epoch-interrupting engine traps tree-sitter's
  `WasmStore`, which never sets a deadline), a 10 ms `EpochTicker` thread kept alive by the host and every extension
  loaded through it, `ceil(deadline / 10 ms) + 1` ticks per call; `load` checks R3 before reading the file, then
  `instantiate` and `WasmExtension::init` (`init-extension`, under the deadline). `WasmExtension::{extension,
  language_server_command, language_server_initialization_options, language_server_workspace_configuration}`; `call`
  hands a worktree resource in and releases it, and `outcome` maps the result.
- `wasm/imports.rs`: `HostState` and every generated `Host*` trait of the 0.6.0 import set. Answered: worktree `id`,
  `root-path`, `read-text-file` (`read_inside`: canonicalised and inside the root), `which` (the worktree's `PATH`,
  executables only, never a name with `/`), `shell-env`; `get-settings` for `language_servers` keyed by server id in
  Zed's `LspSettings` shape; `current-platform`. Refused with `download_refusal`: downloads, GitHub, npm, node,
  `make-file-executable`, `http-client`, `process.run-command`. Installation status is accepted and ignored; `dap`
  resolves to an error; project and key-value-store handles are never handed out.
- `wasm/wasi.rs` + `wit/wasi-0.2.6-subset/`: a hand-written WASI 0.2.6 subset (no `wasmtime-wasi`) — exactly the
  `wasi:{io,cli,clocks,filesystem}` functions a `wasm32-wasip2` `zed_extension_api` component imports: `PWD=/`, one
  empty read-only preopen `/`, discarding stdout/stderr, empty stdin, no terminals, `exit` traps.
- `wit/since_v0.6.0/`: vendored unmodified from zed `c4a82ab1ab958aa43b0c0b15c0bad4fb77a7ca7f` with `LICENSE-APACHE`.
- `index.rs`: `IndexEntry::wasm_enabled` (`#[serde(default)]`), `set_wasm_enabled`. `install.rs`'s `commit` carries
  the previous entry's opt-in over to a new version. `cli.rs`: `ExtensionsCommand::{EnableWasm, DisableWasm}(WasmArgs)`.
- `support.rs`: `load_installed` calls `wasm::served` in place of `server::served` (one line).

## Tests

Acceptance `tests/wasm_command_acceptance.rs` (8) over the fixture code extension `tests/fixtures/wasm-ext/` (source,
`Cargo.lock` and the built 154,907-byte component; rebuild recipe in its README): the WASM's server starts; not
opted in never loads; a configured binary path wins; a download call refuses with the hint; an unsupported API
version is refused with the range; a looping call is stopped at the deadline; enable/disable flip the index entry;
a new version of an opted-in extension stays opted in. Unit: `wasm::tests` (version range, refusal texts,
`served_by_wasm` refusing in the WASM's words and keeping kinds, not-opted-in, nothing read without an unconfigured
server), `wasm::imports::tests` (read confinement against symlink and `..` escapes, `which`, worktree answers,
`get-settings`, the unnamed-server refusal, platform), `wasm::host::tests`, `index::tests`, `cli::tests`.

`language_server_acceptance` was reported aborting with SIGABRT ("mach_msg failed") in about 1 run in 4 during green.
Not reproduced at wrap: 0 of 24 runs at this head and 0 of 24 at the base (`ce1bc5aee`), 12 sequential and 12 four at
a time each. That suite never starts this PR's engine (its servers are not opted in), so it is classified as a
pre-existing, environmental macOS wasmtime flake, not a regression.

## File-length gate and code issues

- New: `wasm/host.rs` 381, `wasm/imports.rs` 378, `wasm/mod.rs` 316, `wasm/wasi.rs` 289 production lines.
- `support.rs` 523 → 523 (the growth of the WASM step moved into `wasm::served`); stack-shared with
  `extension-code-actions` (#682), already over budget before this PR, not split here.
- `install.rs` 395 → 401, `cli.rs` 114 → 134, `index.rs` 76 → 103, `lib.rs` 50 → 58.
- `tddy-tools analyze coverage` + `report` (94 tests, join rate 71.9%): highest CRAP in this PR's code 4
  (`read_inside`, `which`, `choose_server`, `load`, …) once the import tests landed; before them `read_inside` was
  20 and `current_platform` 12, both never executed. No record written; the package has no pre-existing records.

## Left open (backlog)

- [`shell-env` is the whole process environment](../todo/2026-10-23-code-extension-shell-env-is-the-whole-process-environment.md)
  — narrowing it is the developer's decision; the same entry notes that a re-install keeps the opt-in.
