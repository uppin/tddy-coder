# 2026-10-07 — `repoint_facade_imports` names a file's paths by the crate that defines them

Feature: [Rust code restructuring](../rust-code-restructuring.md).

A module that is to move into another crate must name what it uses by the crate that **defines** it, or
the move presents an edge back to the crate it leaves. On one carve, `tddy-session-lifecycle`'s `lib.rs`
re-exported modules of other crates (`pub use tddy_daemon_kernel::config;`), so files wrote
`crate::config::DaemonConfig`; twelve files and thirty lines were re-pointed by hand, four of them
inside a grouped `use` that a text search cannot find. `restructure` now has `repoint_facade_imports`,
and `check --deep` lists what it would rewrite.

- **One file, or one module.** A `symbol` anchor names one file; an `items`/`item` anchor on a `mod`
  declaration names every file of the module. The line carries **only its anchor** — every other field
  is refused as one the operation cannot honour.
- **Named by the defining crate.** Every path whose first hop is a `pub use` of another crate inside the
  file's own crate becomes the path where the item is **defined**: `crate::config::Settings` where
  `lib.rs` holds `pub use kernel::config;` reads `kernel::config::Settings`, in `use` items at any depth
  (groups included) and in bodies. Comments, strings and the crate's own paths are untouched.
- **A grouped `use`.** A group whose members agree on the new prefix has its prefix replaced in place; a
  group whose members need different qualifiers is split — the members that stay keep the group, each
  re-pointed member becomes its own `use`.
- **Refused, naming the path, the file and the line, with nothing written.** A defining crate the
  package does not depend on; a facade that renames an item used in a body; a path spelled across
  whitespace or a comment; a rewrite that would bind a name the scope already binds (`E0252`); a group
  that must be split but carries an attribute or doc comment above it.
- **`check --deep` lists what it would rewrite**, one `file:line: written -> defined` line per path; the
  list is a note, not a finding. The operation is idempotent — a second run rewrites nothing and says
  so — and text-only: no server is asked anything. `verify` accounts for the result with no
  declaration.
