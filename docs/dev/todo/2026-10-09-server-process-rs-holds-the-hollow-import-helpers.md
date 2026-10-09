# 2026-10-09 — `backends/rust/server_process.rs` holds import-tidy helpers beside the server process

**Category:** Deferred refactor (cohesion)
**Source:** #reshape 17/19 (`feature/reshape/rust-backend-split`), discovery Exploration 2 (E2.7), decision F4

`#live-plan` 7/15 (#539) cut `server_process.rs` (92 lines) out of `rust.rs` as one contiguous run.
`without_hollow_imports` and `binds_nothing` (`:21-45`) are the "drop a `use` that binds nothing" pass of the import
repair. They landed beside `Server`, `describe_server_environment` and `default_toolchain_name`, which are about launching
rust-analyzer. `rust-backend-split` put the handshake in a new `handshake.rs` rather than joining this file, partly for
this reason.

## Why deferred

It is not over budget and not in that node's path. Moving it is a two-item `move_item` (to `imports.rs` or
`import_text.rs`, both near their own limits: `imports.rs` is about 411 after the split), which belongs with the next
change to the import pass.

## What would close it

Move `without_hollow_imports` and `binds_nothing` to the import pass's text module (`import_text.rs`, about 318) with
`move_item`, `reexport: none`. Then consider whether `server_process.rs` and `handshake.rs` are one module.
