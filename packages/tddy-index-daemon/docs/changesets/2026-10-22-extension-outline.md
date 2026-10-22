# 2026-10-22 — `extension-outline`: the daemon registers installed extension languages
**Type:** Feature (`#reshape` 20/24, [#679](https://github.com/uppin/tddy-coder/pull/679))

- `main.rs`: `installed_languages()` loads the installed Zed extensions' languages, logging each grammar failure
  once; `wired` returns `anyhow::Result` and the process exits with failure when an extension cannot be read.
- `index.rs`: `WorkspaceIndex::with_supports(Arc<SupportRegistry>)`; `service.rs`:
  `CodeIndexServiceImpl::with_supports` and `build_code_index_entry_with_supports(ports, supports)`, re-exported
  from `lib.rs`. `serve_check` and `anchor_covering` pass the registry as `Options::installed`.
- `queries.rs`: an `Anchors` request on a file only a server-less installed language claims asks no client;
  `anchor_and_span`, `anchored_position` and `wire_range` split out of `anchor_covering`.
- New dependency `tddy-zed-extensions` (wasmtime through tree-sitter's `wasm`): the debug binary went from 59.7 MB
  to 108.3 MB. The allow-list stays Rust's (`TODO(extension-language-server)`).
- Test: `tests/extension_anchors.rs` — `the_daemon_answers_the_same_anchors_as_the_cli`, in process through
  `build_code_index_entry_with_supports`.

Detail: [code_index.CodeIndexService § The language server](../code-index-service.md#the-language-server).
