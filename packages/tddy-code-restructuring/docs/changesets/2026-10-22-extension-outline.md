# 2026-10-22 — `extension-outline`: installed languages reach anchors and the static check
**Type:** Feature (`#reshape` 20/24, [#679](https://github.com/uppin/tddy-coder/pull/679))

- `item_anchor.rs`: `ItemResolver::module_of(root, file)`, provided (default `module_path_of`); `items_anchor`
  prefixes a bare name through it. `registry.rs`: `BackendRegistry::extend`, and `module_of` routed to the backend
  that claims the file (`UnsupportedOp { op: "item anchors" }` without a resolver).
- `runner/options.rs`: `Options::installed: Arc<SupportRegistry>` (empty by default) and
  `Options::is_served_without_a_server(file)`. `restructure_cli::run_with_installed(args, installed)`; `run` passes
  an empty registry; `needs_lsp_client` is false for an `anchors` over a server-less installed file.
- `runner/entry_points`: `registry_for_static(root, installed)` appends the installed static backends;
  `registry_with_installed` does the same after `registry_for`; `item_anchors` needs no client for a server-less
  file; `check_plan`'s static path lowers server-less item anchors (`lower_server_less_item_anchors`) before
  `item_anchor_findings`, so a stale fingerprint is `ItemChanged` and Cargo's prefix rule never sees them.
- `snapshot_resolving` and `apply_held_plan` still build built-in registries (`TODO(extension-language-server)`).
- Tests: `registry::tests` (2) — `module_of` routes to the claiming backend, which answers from the root it was
  built over; `module_of` through a backend with no resolver is refused by name.

Detail: [Item anchors § Languages beyond Rust](../item-anchors.md#languages-beyond-rust).
