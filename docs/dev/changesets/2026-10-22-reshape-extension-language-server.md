# 2026-10-22 — an extension language's server starts from tddy settings, with rename, references and an index (`#reshape` 22/24, #681)

**Type:** Feature

Packages: `tddy-zed-extensions`, `tddy-lsp`, `tddy-code-restructuring`, `tddy-index-daemon`, `tddy-daemon-rpc`,
`tddy-lsp-executor`, `tddy-daemon`. No new external dependency (`tddy-lsp` and `tddy-code-restructuring` are new
dev-dependencies of `tddy-daemon-rpc` and `tddy-lsp-executor`; `tddy-task` of `tddy-zed-extensions`).
Feature docs: [Zed extensions § Language servers](../../ft/coder/zed-extensions.md#language-servers).

## `tddy-zed-extensions`

- `settings.rs`: `ExtensionSettings { lsp, languages }` read from `<extensions dir>/settings.json` (`SETTINGS_FILE`),
  Zed's `lsp.<id>` shape — `ServerSettings { binary, initialization_options, settings }`,
  `BinarySettings { path, arguments, env }` — and `languages.<Name>.language_servers` (`names_server`: a bare id
  names a server; `!<id>` and `...` do not). No file is no settings; an unreadable or unparsable one is
  `ExtensionError::Io` naming it.
- `server.rs`: `ServerChoice { Undeclared, Unconfigured(NoServerCommand), Refused(ServerRefusal), Configured }`.
  `for_language` applies R1–R4: the first manifest entry (by id) naming the language, an `opt_in_languages` language
  only when the settings name the server; the command only from `lsp.<id>.binary.path`, never `PATH`; the
  `language_ids` entry as the launch's `language_id`. `NoServerCommand` reads
  `language server <id> (extension <ext>) has no command: set lsp.<id>.binary.path in <file>`; `ServerRefusal`
  carries a code extension's refusal (K24) as its bare reason. `client_capabilities()` asks for `utf-8` positions
  and `rename`.
- `support.rs`: `ExtensionSupport` carries its `ServerChoice` (`with_server`, `server`); `launch()` is the configured
  server's; `backend()` composes a `ServerBackend` over the live server, one `without_command` for an unconfigured or
  refused server, and the outline backend alone on the static tier. `load_installed` reads the settings once
  (`server::served`), and `supports_of` gives each support its choice.

## `tddy-lsp`

`LaunchSpec::workspace_configuration` (`with_workspace_configuration`); `configuration_answer` answers
`workspace/configuration` item by item from it by `section` path (`null` where absent). `LspClient::initialize`
takes the `LaunchSpec`, and `read_loop` its `Routes`. `fake_lsp` gains
`--requests-configuration <section>` (+ `tddy/configurationAnswers`) and `--answers-rename`.

## `tddy-code-restructuring`

- `backends/server.rs`: `ServerBackend` (`over`, `without_command`) — `rename_symbol` through `textDocument/rename`
  at the item's name position, the `WorkspaceEdit` as the resolution; a server not on `utf-8` positions is refused.
  `RestructureError::ServerNotConfigured { language, reason }`.
- `runner/installed_servers.rs`: `InstalledServers` and `installed_launches_for(plan, installed)`;
  `Options::installed_clients` / `installed_servers()`; `registry_for_waiting` takes the installed servers and
  `registry_for_apply` builds an apply's registry from `Options`; `SupportRegistry::backends_over`. A cold
  `restructure apply` starts one server per installed language its plan's files belong to (`start_servers`).
- `snapshot_resolving` routes through `registry_with_installed`, and asks no server when every item anchor lies in a
  file a server-less installed language claims (`Options::plan_item_anchors_need_a_server`, also asked by the cold
  CLI and the daemon's `Snapshot`). This resolves `extension-outline`'s (#679) `TODO(extension-language-server)`
  markers at `snapshot_resolving`, `store_run::apply_held_plan` and the daemon's `apply_held_plan`, and its
  "allow-list is Rust's alone" note.

## `tddy-index-daemon`

`route_of` / `client_for_route` route definition, references and hover by the file's support; `serves_workspace(root,
supports)`; `readiness.rs` (`Readiness::{ServerStatus, AfterInitialize}`, `index_state_of`), applied by the root's
progress fold too; `Apply` starts the servers its plan needs (`servers_for_a_run`) and sets `Options::installed`.
`ServerNotConfigured` maps to `FailedPrecondition`.

## `tddy-daemon-rpc`, `tddy-lsp-executor`, `tddy-daemon`

`has_an_index`, `CodeNavigationServiceImpl::with_supports`, `code_session::start(.., supports, ..)`,
`warm_for_session_with_supports`, `IndexWarmupObserver::with_supports`, `IndexLspExecutor::with_supports` — every
session-level "does this worktree have an index" asks `serves_workspace` over the host's registry.
`runtime.rs`'s `build` builds that registry once (`code_navigation::host_supports()`, re-exported
`SupportRegistry`) and `BuildContext::index_supports` hands it to `mount_host` and `mount_entries`.

## Tests

Acceptance: `tddy-zed-extensions/tests/language_server_acceptance.rs` (5: rename through the server, cold apply of
a rename through the server started for it, initialization options, `workspace/configuration`, the unconfigured
refusal before anything starts); `outline_acceptance.rs`'s server-less snapshot; `tddy-index-daemon/tests/extension_references.rs` (3: references, an
extension-only worktree's index state, an `Apply` rename through the daemon); `extension_readiness.rs` (2);
`tddy-lsp/tests/workspace_configuration_test.rs` (2). Unit: `settings::tests`, `server::tests`,
`client::dispatch::tests`, `backends::server::tests`, `runner::installed_servers::tests`, `readiness::tests`,
`code_index_warmup::tests`, `index_backed::tests`, index-daemon `lib` tests.

## File-length gate and code issues

- `tddy-index-daemon/src/index.rs` 480 → 605 production lines: stack-shared with `extension-outline` (#679),
  `extension-code-actions` and `extension-wasm-command`, so not split here — recorded as
  `packages/tddy-index-daemon/docs/code-issues/oversized-file-index.md` for after the stack.
- `tddy-zed-extensions/src/support.rs` 457 → 491 (512 before the settings read moved to `server::served`).
- `tddy-daemon/src/runtime/`: +2 lines in `runtime.rs`'s `build`, +10 in `build_context.rs`, +1 each in
  `mount_host.rs` and `mount_entries.rs` — all under budget. `runner/entry_points/store_run.rs` 509 → 501
  (pre-existing, recorded as `oversized-file-runner-store-run.md`).

## Left open (backlog)

- [R1's tie-break](../todo/2026-10-22-extension-language-several-qualifying-servers-tie-break.md) — a decision.
- [The "no code index" wording](../todo/2026-10-22-no-code-index-wording-still-names-only-rust.md).
- [A mixed root reports Rust's progress only](../todo/2026-10-22-mixed-root-index-progress-follows-rust-only.md).
- [Extension servers are not told of tree changes](../todo/2026-10-22-extension-servers-are-not-told-of-tree-changes.md).
