# 2026-10-09 — the apply's tidy does nothing when the compile gate fails

**Category:** Restructure engine missing feature
**Source:** #reshape 3/19 (`feature/reshape/tidy-facades`), decision F2

After #reshape 3, an apply whose compile gate fails says that the tidy did not run and how many `unused import`
warnings the failing check reported in the files it wrote. It still neither removes them nor formats the files: the
author fixes the build by hand, then runs `cargo fix` and `cargo fmt`, as every failed gate of #carve 21/21 did.

**What would close it:** a tidy mode for a broken tree. The current tidy (`runner/tidy.rs`) accepts a round only when
the re-check compiles; on a broken tree the criterion would have to be "the set of errors does not grow", and rustc
may suppress `unused_imports` after resolution errors, so the evidence is weaker. Formatting alone is safe on any
file that parses, but formatting after the gate makes the error lines it already reported stale; formatting before
the gate changes the order every successful run follows.

**Why deferred:** a different algorithm with weaker evidence, chosen against by the developer for #reshape 3 (F2:
say so instead); worth doing only if the hand step stays common once the move nodes of #reshape land.
