# 2026-10-23 — `apply_code_action` runs any language server's code action by name, and extension runs are verified (`#reshape` 23/24, #682)

**Type:** Feature

Packages: `tddy-lsp`, `tddy-code-restructuring`, `tddy-zed-extensions`, `tddy-index-daemon`, `tddy-tools`,
`tddy-tools-cli`. No new external dependency. `tokio` (`rt`) became a normal dependency of `tddy-zed-extensions`
(developer consent; it was already a dev-dependency there and a dependency of `tddy-lsp`). `tddy-zed-extensions`
gained a std-only `testkit` feature, enabled from the dev-dependencies of `tddy-index-daemon` and `tddy-tools-cli`.
Feature docs: [restructure § `apply_code_action`](../../ft/coder/rust-code-restructuring.md#apply_code_action),
[restructure § Files of other languages](../../ft/coder/rust-code-restructuring.md#files-of-other-languages-r5),
[Zed extensions § Language servers](../../ft/coder/zed-extensions.md#language-servers).

## `tddy-lsp`

- `client/commands.rs`: `LspClient::execute_command` → `ExecutedCommand { result, applied_edits }`. Every
  `workspace/applyEdit` the server sends during the call is captured and answered `{applied: true}`. One with no
  `edit` is answered `{applied: false, failureReason}` and is not captured. One that arrives outside a command keeps
  the `null` answer. A per-client `command_gate` serialises commands on a shared client.
- `client/queries.rs`: `pull_diagnostics` (`textDocument/diagnostic`).
- `tests/bin/fake_lsp.rs`: `--code-actions <file>` (with `tddy/applyEditAnswers` and `tddy/codeActionRequests`) and
  `--diagnoses <marker>` (UTF-16 ranges). Doc: `packages/tddy-lsp/docs/commands-and-diagnostics.md`.

## `tddy-code-restructuring`

- `RefactorKind::ApplyCodeAction`; `RefactorOp::action: Option<ActionSelector { title, kind }>`;
  `ActionSelector::wanted()` → `Wanted::{Title, Kind}` holds the "exactly one" rule (R4) for the codec
  (`plan/codec/code_action_fields.rs`) and for selection.
- `backends/code_action.rs`: `CodeActionServer` (implemented for `LspClientBridge`), `select` (R1), `resolve` (R1–R2),
  `OfferedAction`, `CodeActionRefusal { NotOffered, Ambiguous, NoEdit }`. Malformed server answers are `ServerDefect`s.
  `RestructureError::CodeActionRefused` maps to `failed_precondition` in the daemon.
- `backends/server.rs`: `ServerBackend` supports `ApplyCodeAction` (`resolve_code_action` beside `resolve_rename`),
  with `with_code_action_kinds`. `lsp_position` / `lsp_range` refuse a zero line or column. `file_uri_under` is the
  one `file://` spelling.
- `backends/rust/code_action_op.rs`: `RustBackend::apply_code_action` over the shared client; `SUPPORTED` 28 → 29.
- `verify/source.rs`: `SourceVerify`, `SourceChange`, `SourceFinding { NewErrorDiagnostic, ParseError }`, `ParseNode`.
- `runner/comparison.rs`: `Options::verifies`, `changed_sources` (`git diff --relative` plus untracked files),
  `verify_sources(root, options, changed)` (text at `<ref>:./<path>`, so a root below the repository's top works),
  `verify_with_sources`. `Outcome::VerifiedWithFindings`. `console::source_findings` and `console::verify_refusal`
  give one verdict for the cold CLI, `tddy-tools`' warm client and the daemon's single-shot renderer.

## `tddy-zed-extensions`

- `verify.rs`: `ExtensionVerify` (`new`, `for_languages`, `impl SourceVerify`). Diagnostics are read by pull after
  each text is announced, and the document is closed on every path. A server with no `diagnosticProvider` is a
  `ServerDefect`. `new_error_diagnostics` carries ranges through the lines a change left alone. `parse_errors`
  walks the tree with a cursor.
- `support.rs`: `ExtensionSupport::parse`, `document_language_id`, `with_code_action_kinds` (from the manifest entry,
  passed on to the `ServerBackend`); `InstalledExtensions::register_installed`.
- `testkit.rs` (feature `testkit`): `install_demo_toml`, `declare_in_demo_toml`, `commit_baseline`.

## `tddy-index-daemon`

- `Verify` builds `Options::verifies` over each touched language's warm server (`queries/verify.rs`) and answers
  `VerifyResponse.source_findings` (proto field 9). `holds` is false when there is any finding.
  `WorkspaceIndex::with_verified_languages` / `CodeIndexServiceImpl::with_verified_languages`;
  `host_languages_launching` → `HostLanguages { supports, extensions }`.
- `index/routing.rs` (new): `Route`, `ServedFile`, `route_of`, `client_for_route`, `holds_a_server_for`,
  `warmth_of`, moved out of `index.rs` by the engine (see below).

## `tddy-tools` / `tddy-tools-cli`

- The cold `restructure verify` builds the verifies of the installed languages (`ColdLanguages { supports,
  verifies }`, parse only: no server is started). The warm client renders `source_findings`.

## Decisions

- `only` for an extension server is the manifest entry's `code_action_kinds`; empty asks for every kind.
- Several command edits stay separate `FileEdit::Change`s, in arrival order.
- An action with both an edit and a command takes the edit and does not run the command
  ([backlog](../todo/2026-10-23-apply-code-action-transport-gaps.md) § 3).
- Findings make verify not hold, on every front end.

## Restructuring (file-length gate)

Production lines by the items rule (`tddy-tools restructure lines`), merge base → HEAD:

| File | Base | HEAD | How |
|---|---:|---:|---|
| `tddy-index-daemon/src/index.rs` | 605 | 495 | engine moves: `move_item` (`Route`, `ServedFile`, `name: routing`), then `move_impl_members` (`route_of`, `client_for_route`, `holds_a_server_for`, `warmth_of`); `holds_a_server_for` widened to `pub(super)` |
| `tddy-index-daemon/src/index/routing.rs` | — | 148 | new |
| `tddy-index-daemon/src/queries.rs` | 492 | 442 | engine move: `move_item` (`serve_verify`, `comparison_against`, `with_extension_verifies`, `name: verify`) |
| `tddy-index-daemon/src/queries/verify.rs` | — | 114 | new |
| `tddy-zed-extensions/src/support.rs` | 491 | 536 | **not split: stack-shared** with #683 (K24). Recorded as `packages/tddy-zed-extensions/docs/code-issues/oversized-file-support.md` |
| `tddy-code-restructuring/src/backends/rust.rs` | 2,947 | 2,953 | wiring only (`mod code_action_op;`, `SUPPORTED`, one `resolve_opening` arm). Exempt and owned by its own backlog entry; measurement row added to `oversized-file-backends-rust.md` |

`restructure verify --against HEAD` after each plan: 421,742 statements before and after, every one accounted
for. The comment-line multiset of `tddy-index-daemon/src` was equal before and after.

**Code issues closed:** `packages/tddy-index-daemon/docs/code-issues/oversized-file-index.md` (605 → 495, final
measurement above) and its backlog entry `2026-10-06-split-tddy-index-daemon-index-rs.md`
("`tddy-index-daemon/src/index.rs` crossed the file-length budget"), both deleted.

## CRAP pass (`tddy-tools analyze coverage` + `report`, per touched crate)

Functions this PR added or made worse were fixed in place:

- `tddy-index-daemon` `render::verify`: 20 at the merge base, 30 here (complexity 5, unexecuted).
- `tddy-tools` `index_console::verify`: 12 at the base, 20 here.
- Both now emit what a pure `verify_report` decides, and that function is tested for a finding and for a clean hold.
- `tddy-code-restructuring` `plan::codec::parse_op`: complexity 50 → 51 from the `code_action_fields::rules` call. An engine `extract_method` (`operation_rules`) takes the three operation-rule calls out of it.

These stay as they were at the base and are not this PR's to fix:

- `backends::server::workspace_edit_of` (56) and `restructure_cli::run_with_extensions` (42) carry the base's numbers. The second is the base's `run_with_installed` body, moved.
- Their low coverage is the per-crate capture's blind spot: they are exercised from `tddy-zed-extensions` and `tddy-tools-cli`.

No code-issue record was written.

## Left open (backlog)

- [Extension verify reads new error diagnostics only on the warm daemon](../todo/2026-10-23-extension-verify-diagnostics-only-on-the-warm-daemon.md)
- [Extension verify refuses a push-only server](../todo/2026-10-23-extension-verify-refuses-a-push-only-server.md)
- [`apply_code_action` gaps](../todo/2026-10-23-apply-code-action-transport-gaps.md): a self-spawned Rust backend,
  `ContentModified` retry, edit-plus-command actions

## Verification

Scoped to the six touched packages (`./test -p tddy-lsp -p tddy-code-restructuring -p tddy-zed-extensions
-p tddy-index-daemon -p tddy-tools -p tddy-tools-cli`): 2,663 passed, 0 failed, 20 ignored. One process abort
(SIGABRT) in `language_server_acceptance` (`a_configured_server_renames_an_extension_symbol`), during a concurrent
cargo build. It did not recur in eight re-runs (the suite three times, the test alone five). Baseline before the
wrap's cleanups (`ce1bc5aee`): 2,661 passed, 0 failed. `cargo clippy -p <the six> --all-targets -- -D warnings`
and `cargo fmt --check -p <the six>`: clean. Whole-workspace health is CI's.
