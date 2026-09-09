# 2026-09-09 — Import restoration reads the parent's own `use` tree

**Type:** Fix

Six defects found by relocating every `impl` block out of an 16,659-line module. Each presented as
something it was not, which is what made them expensive rather than difficult.

**Restoration could not reconstruct two kinds of binding.** rust-analyzer offers `Import` for items
by their canonical path — so a name the parent binds under an alias
(`ProbeOutcome as ProtoProbeOutcome`) got the unaliased path, which binds nothing, and a **module**
binding (`use crate::tool_engine;`) got nothing offered at all. The first refused, blocking every seam
naming a generated proto type; the second passed silently and left three modules referencing an
unlinked crate. Both are now read from the parent's own `use` tree, which already says what the moved
code meant.

**A contested name is settled on module agreement** where an exact binding no longer exists — routine
once a seam has moved the code that used a name and the pass has pruned the parent's now-unused
binding. A file importing twenty-six names from `tddy_service::proto::connection` and one contested
`Signal` meant that one. Two candidates from two imported modules is still refused.

**The pass no longer defeats the facade it was given.** It ran before the facade was written, so the
server reported a moved name as unresolved and it added a *private* named import through the new
module — which shadowed the `pub use` added moments later, leaving the facade inert and an outside
caller on `E0603`. And a facade is emitted at the widest visibility the relocated items carry, because
the assist rewrites what it relocates to `pub(crate)` and a `pub` glob over none of it re-exports
nothing, which `-D warnings` turns into a build failure.

**A rewrite written over itself is refused.** One run produced
`seeded_clone_guard::SeededCloneGuardloneGuardloneGuard` — the identifier's tail inserted twice at a
four-character offset — once among some twenty rewrites, and reported success. Every `module::Ident`
the assist writes must now name something the seam moved. The residual-placeholder check could not
see this: it counts placeholders, not rewritten references.

**Two limits were caps rather than guarantees.** `IMPORT_PASSES` was 64 on the stated premise that the
count needed is "far below this" — false for a 6,484-line `impl` of a generated gRPC trait, which needs
one import per proto type it names. And the per-operation settle budget was a hardcoded 30s that
`--indexing-budget` could not reach, so the documented remedy for a slow machine was inert.

**Diagnostics, which is where the time actually went.** A wrong assist title, an unrefactorable range,
a server told nothing about its client, and a server that could not yet *type* the range all produced
byte-identical output. An absent assist now names the titles the server offered; an expired budget
distinguishes a range that does not support the assist from a server that cannot type it, by probing
hover at the range; a timeout says how far the index got, since the server's last line is routinely a
sub-step carrying no percentage; and `apply` reports each operation as it commits, after the commit, so
a line on stdout means the edit is on disk and in the journal.

Feature doc: [rust-code-restructuring.md](../../../../docs/ft/coder/rust-code-restructuring.md).
248 tests, 0 failing; 26 added across this package and `tddy-lsp`.
