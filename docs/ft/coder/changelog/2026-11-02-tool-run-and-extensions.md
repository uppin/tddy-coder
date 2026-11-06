# 2026-11-02 — A tool definition declares how it runs, and owner-validated extensions

`#agent-tools` 2/12 ([#791](https://github.com/uppin/tddy-coder/pull/791)).

- **`run:`** holds an argv template (literals with `{name}` placeholders, `if`/`then`/`else`
  segments, `splice` lists), static `env` and a `transport` (`pty` or `stdio`). An unbound name is
  refused, never rendered as empty.
- **`extensions:`** is an open map whose keys name an owner. A key no owner claims, or a section its
  owner refuses, fails the load naming the file. `coding_agent` is the first owner's key.
- **`tddy-tools` is a built-in tool**, which coding agents list in `deps`.

See [Sandbox builder § Declared tools](../sandbox-builder.md#declared-tools).
