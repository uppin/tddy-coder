# 2026-09-22 — The session storage layer becomes `tddy-session-store`, and SQLite leaves `tddy-core` with `tddy-session-catalog`

**Type:** Refactor

`#carve` 7/11, PR [#493](https://github.com/uppin/tddy-coder/pull/493).

**35 crates depended on `tddy-core`, and every one of them compiled `sqlx` with a bundled C
SQLite.** Four files named `sqlx`, 854 lines, all in `tddy-core/src/session_catalog/`. The catalog
could not leave alone. It depends on `session_actions`, which depends on `output` and
`atomic_file`, and those depend on `error`. So the whole storage layer moved, leaf-first, into two
new crates:

| Crate | Holds | Workspace dependencies |
|---|---|---|
| [`tddy-session-store`](../../../packages/tddy-session-store/README.md) | `atomic_file`, `error`, `output`, `session_actions` (all but `session_dir.rs`) | `tddy-workflow`, `tddy-actions`, `tddy-task` |
| [`tddy-session-catalog`](../../../packages/tddy-session-catalog/README.md) | the per-session catalog, and `sqlx` | `tddy-session-store`, `tddy-task` |

`tddy-core` keeps a glob facade at each of the four storage paths, so no consumer of those was
edited. `tddy_core::session_actions` also keeps `session_dir.rs`. The catalog has **no** facade.
Its consumers in `tddy-coder` and `tddy-bsp` name `tddy_session_catalog` directly, and a doc
comment in `tddy-semantic-index` plus two `tddy-model-registry` comments were repointed. Its two
test binaries moved with it. `tddy-core`'s manifest drops `sqlx`, `regex` and `tddy-actions`. It
keeps `jsonschema`, because `session_action_pipeline.rs` stays and still names it.

Nothing changes behaviour. Every moved body is byte-identical under `diff` against the base, apart
from the necessary lines (`use` paths, module wiring, the `runtime` visibility below, and doc
comments that named the old owner). Log targets still read `tddy_core::session_actions::…`, so
existing log filters keep matching.

## Premises corrected at green

The developer decided 1–4 during `/green`. 5 came out of validation.

1. **No `tddy_core::session_catalog` facade.** The plan's FR2 and AC5 assumed one. A facade would
   make `tddy-core` depend on the catalog, so on `sqlx`, and every dependent would keep compiling
   SQLite, which defeats the node. The catalog's consumers are edited instead, and AC5 excludes
   `session_catalog`.
2. **AC2 re-targeted.** `cargo tree -p tddy-coder -i sqlx` could never come back empty, because
   `tddy-coder` opens the catalog pool itself. AC2 measures `tddy-workflow-recipes` and `tddy-tui`
   instead. Both are `tddy-core` consumers that never open a catalog.
3. **`session_dir.rs` stays in `tddy-core`.** The storage group was not a closed DAG. Discovery
   missed `session_actions/session_dir.rs → changeset` (`read_changeset`, since #474), and that
   edge reaches the workflow SCC. `tddy_core::session_actions` is therefore a facade that also
   defines `list_actions_in_session_dir`, `invoke_action_in_session_dir` and `ListActionsResponse`.
4. **The AC3 allowlist is widened.** Discovery also missed `session_actions/runtime.rs →
   tddy-actions, tddy-task` (since #244). Neither depends on `tddy-core`, so the storage crate takes
   both. AC3 allows exactly `tddy-workflow`, `tddy-actions` and `tddy-task`, and matches exact
   crate names. The planned `starts_with("tddy-workflow")` would also have admitted
   `tddy-workflow-recipes`, which depends on `tddy-core`. AC3 also asserts that no allowlisted crate
   reaches `tddy-core` transitively.
5. **`session_actions::runtime` is `#[doc(hidden)] pub`.** It was `pub(crate)`.
   `tddy_core::session_action_jobs::runner` stays, because it needs `read_changeset`, and it now
   reaches the runtime across a crate boundary. That exposes **seven** functions, not the two the
   plan counted, also as `tddy_core::session_actions::runtime::…`: `session_task_registry`,
   `start_manifest_async`, `schedule_async_log_mirror`, `task_status_for_job`,
   `cancel_task_in_registry`, `block_on` and `write_channel_logs`. None of them is API, and
   `block_on` panics on a current-thread runtime.

Two more corrections belong to the stack rather than to this node. `#carve` 5/11 → 7/11 is a real
edge: `error.rs`'s `ClarificationQuestion` import is a behaviour dependency. `#carve` 3/10 → 7/11
is not an edge, because the group is a DAG, so leaf-first ordering needs no cluster support.
`move_module_to_crate` was not usable for these facade-shaped moves, so every phase was done by
hand with `git mv`.

## Who still compiles `sqlx`

Measured with `cargo tree --workspace -i sqlx -e normal`. **44 of `tddy-core`'s 52 transitive
dependents no longer compile `sqlx`.** The 8 that still do all reach a crate that really opens a
database:

| Crate | Path to `sqlx` |
|---|---|
| `tddy-bsp` | direct: `tddy-session-catalog` |
| `tddy-coder` | direct: `tddy-session-catalog` (worktree-open populate), and through `tddy-bsp` |
| `tddy-tools` | through `tddy-bsp` |
| `tddy-model-registry` | direct: its own `sqlx` store, and through `tddy-coder` |
| `tddy-session-lifecycle`, `tddy-daemon` | through `tddy-coder` / `tddy-bsp`, and through `tddy-model-registry` |
| `tddy-demo`, `tddy-desktop` | through `tddy-coder` / `tddy-daemon` |

## Code issues closed

Claimed by this PR, re-measured at wrap, and its file is deleted.

| Record | At detection (2026-09-15) | At wrap (2026-09-22) |
|---|---|---|
| `tddy-core`: `heavy-dependency-sqlx-session-catalog` | 4 files in `tddy-core/src` name `sqlx` · 854 lines · 35 dependent crates compile it · bundled SQLite through the `sqlite` feature · `sqlx` a direct dependency | `grep -rl sqlx packages/tddy-core/src \| wc -l` → **0**. `cargo tree -p tddy-core -i sqlx` → *"package ID specification `sqlx` did not match any packages"*, so `sqlx` is nowhere in `tddy-core`'s graph. `cargo tree -p tddy-workflow-recipes -i sqlx` and `-p tddy-tui -i sqlx` → the same, no match. **8** of 52 `tddy-core` dependents still compile it, each through a real catalog or database user (table above). The 854 lines and 4 `sqlx` files are now `tddy-session-catalog`'s |

## Code issues moved, opened, unchanged

- **Moved:** none. No open record named a file under `atomic_file.rs`, `error.rs`, `output/`,
  `session_actions/` or `session_catalog/` apart from the one closed above.
- **Written at validation:** `tddy-model-registry`, `oversized-file-store` (a first detection by
  the file-length gate), and a measurement row on `tddy-coder`, `oversized-file-run`. Neither file
  grew in this PR, which touched a doc comment in one and a `spawn_session_catalog_populate` import
  path in the other.
- **Left alone:** `tddy-core`'s `god-object-presenter` (claimed by #495) and
  `cycle-dto-inside-behaviour-module` (unclaimed). This PR did not touch the code either describes.

## Backlog

No `docs/dev/todo/` entry is resolved here. Two were read during the work and left open:
`2026-07-01-tddy-core` (pre-existing `tddy-core` notes) and
`2026-09-10-the-dependency-boundary-harness-is-duplicated-per-crate`. For the second: both new
crates' boundaries are pinned by one suite, `tddy-core/tests/session_store_shape.rs`, which adds
one more manifest-parsing, closure-walking copy of that harness. It handles `[dependencies.<name>]`
table headers, one of the blind spots the entry lists. Lifting the harness into
`tddy-testing-commons` remains that entry's work.

## Verification

Scoped to the packages touched: `./test -p tddy-core -p tddy-session-store -p tddy-session-catalog
-p tddy-bsp` → **606 passed, 0 failed** (baseline 603, plus the 3 contract tests that were red).
`clippy --all-targets -D warnings` on the same four plus `tddy-coder` is clean, and so is
`cargo fmt --all --check`. `tddy-core/tests/session_store_shape.rs` pins the result:

- `tddy-core` declares neither `sqlx` nor `libsqlite3-sys`, and keeps `jsonschema`.
- `tddy-session-store` depends on exactly its three-crate allowlist, and none of those reaches
  `tddy-core`.
- `tddy-session-catalog` owns `sqlx`, depends on `tddy-session-store`, and has no edge back to
  `tddy-core`.

The suite parses dependency tables into crate names. It does not match substrings, so a comment or
a `package =` rename cannot satisfy it. Each check was shown to fail against a mutated,
rule-violating manifest.
