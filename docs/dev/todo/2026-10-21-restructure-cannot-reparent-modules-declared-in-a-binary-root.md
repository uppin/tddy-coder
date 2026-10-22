# 2026-10-21 — `restructure` cannot re-parent a module declared in a binary root

**Category:** Future enhancement
**Source:** `#satellite` 15/15 (PR #690), the six binary-only modules of `tddy-tools` re-parented
from `src/main.rs` into the package's own `src/lib.rs`
**Related:** [2026-10-21-restructure-cannot-move-a-binary-root-or-a-test-crate.md](2026-10-21-restructure-cannot-move-a-binary-root-or-a-test-crate.md)
covers moving the binary root itself and a `tests/*.rs` suite **between packages**. This entry is the
case inside one package: a module the binary root declares, moved under the library root.

A package with both a library and a binary has two crate roots under one package name:

```text
packages/tddy-tools/
  src/lib.rs      # the library root: `pub mod server;` …
  src/main.rs     # the binary root: `mod cli;` …
  src/cli.rs      # declared by main.rs only; reaches the library as `tddy_tools::server`
```

```rust
// packages/tddy-tools/src/main.rs (binary root)
mod cli;
use tddy_tools::server::PermissionServer;

// packages/tddy-tools/src/lib.rs (library root)
pub mod server;
```

The wanted move is `mod cli;` out of `main.rs` and `pub mod cli;` into `lib.rs`, with the binary's
callers re-pointed to `tddy_tools::cli` and `cli.rs`'s own `tddy_tools::` paths respelled `crate::`.

`restructure anchors packages/tddy-tools/src/main.rs --items cli` emits the anchor
(`"items":["tddy_tools::cli"]`), but a `reparent_module` with `"to":"tddy_tools"` is refused by
`check` and `check --deep` alike:

```text
0: `tddy_tools` is in packages/tddy-tools/src/lib.rs, and the anchor is in packages/tddy-tools/src/main.rs: the anchor must be on the `mod` declaration, in the file of the module that declares it
0: `tddy_tools` declares no `mod cli;`: `reparent_module` is anchored on the module's declaration
Error: 12 finding(s) — see above. Nothing was written.
```

(The same two findings repeat for `index_client`, `index_console`, `pty_relay`, `remote_cli` and
`session_hook`.) The engine resolves a package name to `src/lib.rs` only, so the binary root and
everything it declares cannot be addressed as a parent: there is no spelling of `to` or of the
anchor's parent that names `src/main.rs`. Changing `reexport` does not help, because the refusal is
in resolving the anchor, not in the facade.

The node did the re-parent by hand with the developer's consent: six `pub mod` lines in `lib.rs`, the
six `mod` lines removed from `main.rs` with one `use tddy_tools::{cli, …};` added, and three
`tddy_tools::` → `crate::` path fixes in `cli.rs`. The module files did not move.

**Hand fixes after the move** (the widening is marked `TODO(restructure-engine)` in code):

| File | Line | Item | Change |
|---|---|---|---|
| `packages/tddy-tools/src/main.rs` | 13 | `use tddy_tools::{cli, index_client, pty_relay, remote_cli, session_hook};` | added (replaces the six `mod` lines; `index_console` is reached only by `index_client`) |
| `packages/tddy-tools/src/cli.rs` | 19, 486, 606 | `session_actions_cli`, `server::PermissionServer` (×2) | `tddy_tools::` → `crate::` |
| `packages/tddy-tools/src/index_client.rs` | 39 | `run_restructure` | `pub(crate)` → `pub` (E0603: the binary is a separate crate now) |

**What would close it:** a way to name a package's binary root as a parent (for example a
`<package>::<bin-name>` crate path, or reading the anchor's own file as the old parent when it is a
binary root), so `reparent_module` can move a `mod` from `src/main.rs` (or `src/bin/*.rs`) to the
library root, respell the moved files' `<package>::` paths as `crate::`, and re-point the binary.

**Marked in code:** `TODO(restructure-engine)` at the two refusals in
`packages/tddy-code-restructuring/src/backends/rust/module_reparent/survey.rs`, and above the widened
`run_restructure` in `packages/tddy-tools/src/index_client.rs`.

## Hit again by `#satellite` 16/16 (the sweep, 2026-10-22): `build.rs`

`packages/tddy-service/build.rs` (744 production lines, `main` alone 682) is a crate-root file under no
`src/` directory, so the same resolution rule refuses it: `build.rs is not under packages/tddy-service/src, so
it is no module of the package`. The sweep cut `main` by hand, with the developer's consent, into
`build/{core_services,unbundled_families,platform_services,descriptors}.rs` (`#[path]`-included from
`build.rs`, bodies byte-identical, every function at most 57 lines), proved by a `diff -r` of the generated
`OUT_DIR` before and after. `TODO(restructure-engine)` at the head of `build.rs`. The engine would need to
treat a package's `build.rs` as a crate root of its own.
