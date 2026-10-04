# 2026-10-04 — The daemon registers the restructure executor when it manages an index

**Type:** Feature

`#live-plan` 14/15, PR [#573](https://github.com/uppin/tddy-coder/pull/573). Cross-package entry:
[2026-10-04-session-restructure-tools.md](../../../../docs/dev/changesets/2026-10-04-session-restructure-tools.md).

`build` registers `IndexRestructureExecutor` with `register_restructure_executor`, from the same
`IndexChannel` as the `Lsp*` executor, when `index_daemon:` is configured; without the section nothing is
registered. See [daemon-endpoint.md](../daemon-endpoint.md).
