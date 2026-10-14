# 2026-10-08 — a cross-crate move cannot create its destination crate, and copies no crate named only by an attribute macro or a target table

**Category:** Engine failure plus manual fixes (a new crate skeleton and manifest lines), developer-consented 2026-10-08
**Source:** #carve 21/21 (PR #536), R7: `cli_session_manager` (anchor) with nine directory children and `session_toolcall` → new crate `tddy-cli-sessions`. The entry first recorded the children named in `also` staying behind and their crates missing from the manifest; both are closed, and what remains is below. Engine causes already filed:
[2026-10-04-restructure-move-cluster-to-crate-leaves-a-modules-directory-children-behind](2026-10-04-restructure-move-cluster-to-crate-leaves-a-modules-directory-children-behind.md)
(`libc` from a target table),
[2026-10-08-restructure-move-to-crate-misses-a-crate-named-only-by-an-attribute-macro](2026-10-08-restructure-move-to-crate-misses-a-crate-named-only-by-an-attribute-macro.md)
(`async-trait`)

## What remains

1. **The destination crate must exist.** A plan whose `to` is not a crate is refused
   (`is not a crate: …/Cargo.toml could not be read`). R7 wrote the crate skeleton by hand: `Cargo.toml`,
   `src/lib.rs` and a `members` line in the root `Cargo.toml`. The engine gains the dependency edges once
   the crate exists.
2. **A crate only an attribute macro names** (`async-trait` through `#[async_trait]`) is not in the
   destination's manifest: the path survey reads paths, and an attribute is not one.
3. **A crate the origin declares under a target table** (`libc` under `[target.'cfg(unix)'.dependencies]`)
   is not copied: the dependency lookup reads `[dependencies]` and `[dev-dependencies]` only.

## What the engine should do

Offer a create-crate step so a destination need not exist, read attribute names for the crates they come
from, and read the origin's target tables. Delete this file with those fixes.

## Why deferred

All three are the `new-crate` node's (`feature/reshape/new-crate`, `#reshape` 8/19). `#reshape` 5 carries
directory children and feeds the crates they name through the same manifest pass as the module's own file.
