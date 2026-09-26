# Initial Discovery: Cross-crate move paths

**Changeset**: [2026-09-26-restructure-move-paths.md](./2026-09-26-restructure-move-paths.md)
**Date**: 2026-09-26
**Passes**: 2 (Exploration 1 is the stack's whole-work discovery, copied in full)

## Combined Conclusions

**Packages in play:** `tddy-code-restructuring` (plan schema, anchors, ledger, runner, rust
backend), `tddy-index-daemon` (warm index, `code_index.proto`, apply loop), `tddy-tools`
(`restructure` CLI front end, via `restructure_cli.rs` in `tddy-code-restructuring`), skill
`.agents/skills/code-restructuring/` (authoring rules).

**State A — the plan.** `plan.rs`: JSONL, line 1 `{"v":1,"snapshot":{path: "sha256:…"}}`, every
later line a `RefactorOp { op, anchor, name, to, variant, with_private_deps, reexport, to_file,
also }`. `Anchor` has two kinds:

- `Symbol { file, path }` — `path` is **a bare name**, resolved by `find_symbol`
  (`backends/rust.rs:2226`), a depth-first walk of `textDocument/documentSymbol` returning the
  **first** node whose `name` equals it. No qualification, so `new` in a file with two `impl`s is
  whichever comes first. Not a fully-qualified path; not crate-rooted.
- `Range { file, start, end }` — absolute 1-based line/col, **trusted exactly** by
  `anchor_range` (`rust.rs` ~1756) and `rename_symbol`.

Anchors are "original snapshot coordinates". Within **one run**, `PositionLedger` (`ledger.rs`)
folds every applied edit and translates later anchors, refusing (`AnchorInvalidated`) when an
endpoint fell into removed text. The ledger is checkpointed per op (`LedgerCheckpoint`) beside the
journal. **The JSONL file itself is never updated**: after a run, or across plans in a multi-plan
carve, every remaining range anchor on disk is stale. `verify_snapshot` refuses on hash drift;
`restructure snapshot` rewrites only the header (`rehashed_header`), leaving anchors wrong — the
recorded todo `2026-09-24-restructure-snapshot-cannot-rebase-a-stale-plan.md` (six stale plans on
#524, re-anchored by a throwaway difflib script).

**State A — authoring.** The skill mandates `restructure anchors <file> --items A,B` ("Do not
hand-write line numbers"). `places_of` (`rust.rs:1970`) matches `--items` against the outline and
the code-issue `broken-restructure-anchors-empty-outline.md` records it **resolves nothing** — the
outline comes back empty (warm: refused in 3–89 ms; cold: `lsp server exited`). Unclaimed.

**State A — the daemon.** `tddy-index-daemon` serves `code_index.CodeIndexService`
(`proto/code_index.proto`): `Warm`, `Check`, `Apply`, `Anchors`, `PlanStatus`, `Verify`, analysis
RPCs, `Workspaces`. Requests name a plan by **path** (`CheckRequest.plan`, `ApplyRequest.plan`,
`PlanStatusRequest.plan`). The daemon holds per-root state (`index.rs`: `WorkspaceIndex`, a per-root
queue, `LspRegistry`) but **no plan state**: `apply.rs::apply_plan` does
`Plan::parse(&read_to_string(options.plan()?)?)` on every call and uses
`StatePaths::under(root)` — the repo-scoped journal the code issue
`stale-repo-scoped-restructure-state-apply.md` records (Claimed by: nobody; should be
`StatePaths::for_plan`). One code path for single-shot (in-process) and served (`--grpc`/`--stdio`)
modes; served mode shuts down on `^C`/`SIGTERM` (`serve.rs:182`) with `servers.shutdown_all()` —
the natural hook for a flush-on-exit. `tree_changes.rs` already tracks file changes for
`workspace/didChangeWatchedFiles`.

**Collisions:** open `#carve` PRs #531–#536 touch **none** of `tddy-code-restructuring`,
`tddy-index-daemon`, `tddy-lsp` (checked per PR file list). `docs/dev/1-WIP/2026-09-17-restructure-
refusal-truth-and-authoring-gates.md` is an older WIP changeset in the same packages with no open PR
found — check its status before node 1's changeset lands.

**Backlog (docs/dev/todo) in the path:** 23 restructure/index entries. Directly in scope:
`snapshot-cannot-rebase-a-stale-plan` (answered by this stack), `extract-module-cannot-see-sibling-
seams-in-one-plan` (plan-level anchoring of a keyword an earlier op writes — likely answered by
re-resolving LSP paths between ops). Extraction-anchor defects (same code path as range resolution):
`extract-variable-waits-forever-on-a-range-opening-with-a-borrow`, `extract-variable-hoists-a-
borrowed-field-by-value`, `extract-method-accepts-a-return-before-a-unit-if-tail`,
`extract-method-leaves-a-function-local-use-behind`. Cross-crate move defects (8, `crate_move.rs`)
and `check` misses (2) are a different surface.

## Exploration 1: plan schema, anchor resolution, daemon apply loop — 2026-09-26

**Agent**: parent Grep/Read (Bash)
**Scope**: `tddy-code-restructuring` plan/ledger/backend, `tddy-index-daemon` service/apply/proto,
code-issues, backlog, open PR collisions.

### Sequence

1. `ls docs/dev/1-WIP/`, `ls` of `src/` for the six candidate packages, `ls packages/*/docs/code-issues`,
   `ls docs/dev/todo | grep restruct|index|lsp|anchor|plan` — orient.
2. Read `docs/dev/todo/2026-09-24-restructure-snapshot-cannot-rebase-a-stale-plan.md` — the stale-plan record.
3. `wc -l` over both packages' `src/*.rs` — size map.
4. Read `.agents/skills/planning/references/initial-discovery.md` — dump contract.
5. For each restructure todo: title, Category, "What would close it" head — backlog gist.
6. Grep `pub (enum|struct|fn)` in `plan.rs`; read `plan.rs` 1–60, 160–330, 400–440.
7. Read `ledger.rs` 1–80; grep `Anchor::Symbol` across both packages.
8. Read `tddy-index-daemon/src/lib.rs` 1–60, `service.rs` 1–80, `apply.rs` 1–70.
9. Read `backends/rust.rs` 1740–1830; grep `fn locate_symbol` -A40.
10. Grep `rpc|message|service|plan` in `proto/code_index.proto`; read lines 150–185.
11. Grep `fn find_symbol` -A30 in `backends/rust.rs`.
12. Grep shutdown/signal in `serve.rs`/`main.rs`; read `index.rs` 1–40; grep `Claimed by` in code-issues.
13. Grep `anchor|jsonl|plan` in `.agents/skills/code-restructuring/SKILL.md`; read WIP changeset head;
    `gh pr list` filtered for restructure/index/lsp/carve.
14. Read both relevant code-issue records; `gh pr view <n> --json files` for #531–#536.
15. Grep `TDDY_INDEX_SOCKET` in `restructure_cli.rs`; read `places_of`; `ls` both `tests/` dirs.

### Grep / glob

| Tool | Pattern / glob | Path scope | Notable hits |
|------|----------------|------------|--------------|
| Grep | `Anchor::Symbol` | both packages `src` | `ledger.rs:123`, `registry.rs:256`, `crate_move.rs:342`, `runner/budget.rs:123`, `crate_move/cluster.rs:1229`, `backends/rust.rs:1762,1800` |
| Grep | `rpc` | `code_index.proto` | `Warm`, `Check`, `Apply`, `Anchors`, `PlanStatus`, `Verify`, `Coverage`, `Report`, `DuplicateTests`, `Complexity`, `Workspaces` |
| Grep | `Claimed by` | code-issues | only `stale-repo-scoped-restructure-state-apply.md:9` — `nobody` |
| Grep | `shutdown\|signal` | daemon `serve.rs`, `main.rs` | `serve.rs:78,80,182–186` ctrl_c/SIGTERM; `main.rs:104` `servers.shutdown_all()` |
| gh | PR files `#531..#536` | restructure/index/lsp paths | 0 files each |

### Inspected files

#### `packages/tddy-code-restructuring/src/plan.rs`

```rust
/// Where an operation applies. Anchors are always expressed in *original snapshot* coordinates;
/// the [`crate::PositionLedger`] translates them to current coordinates at execution time.
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Anchor {
    /// A named symbol — survives edits above it, so preferred where the operation allows it.
    Symbol { file: String, path: String },
    /// A source range — required by extractions, which act on statements rather than a symbol.
    Range { file: String, start: Position, end: Position },
}

pub struct Plan {
    pub version: u32,
    /// Content hash per file the plan touches, taken when the plan was written.
    pub snapshot: BTreeMap<String, String>,
    pub ops: Vec<RefactorOp>,
}
const SCHEMA_VERSION: u32 = 1;

pub fn parse(jsonl: &str) -> Result<Plan>            // line 1 header, rest ops; refuses text/code/content fields
pub fn rehashed_header(&self, root) -> Result<String> // what `restructure snapshot` writes — header only
pub fn verify_snapshot(&self, root) -> Result<()>      // SnapshotMismatch on drift
```

#### `packages/tddy-code-restructuring/src/ledger.rs`

```rust
//! The position ledger: a projection that maps *original snapshot* coordinates to current
//! on-disk coordinates. ... repo-wide and folds the **entire** edit set of every operation
pub struct PositionLedger { files: BTreeMap<PathBuf, Vec<AppliedEdit>>, renames: BTreeMap<PathBuf, PathBuf> }
pub struct LedgerCheckpoint { pub op: usize, pub ledger: PositionLedger }
/// Returns AnchorInvalidated when the position fell inside text an earlier operation removed
pub fn translate(&self, file: &Path, pos: Position) -> Result<Position>
```

#### `packages/tddy-code-restructuring/src/backends/rust.rs`

```rust
fn anchor_range(&mut self, uri: &str, op: &RefactorOp) -> Result<Range> {
    match &op.anchor {
        Anchor::Range { start, end, .. } => Ok(Range { start: *start, end: *end }),
        Anchor::Symbol { path, .. } => {
            let position = self.locate_symbol(uri, path)?;   // textDocument/documentSymbol, polled
            ...

fn find_symbol(symbols: &Value, name: &str) -> Option<Value> {
    for symbol in symbols.as_array()? {
        if symbol.get("name").and_then(Value::as_str) == Some(name) {
            return symbol.pointer("/selectionRange/start")....cloned();   // FIRST match wins
        }
        if let Some(found) = symbol.get("children").and_then(|kids| find_symbol(kids, name)) { return Some(found); }
    }
    None
}

fn places_of(outline: &[OutlineItem], items: &[String], file: &str) -> Result<Vec<usize>> {
    // `{item}` is not an item `{file}` defines at module level   ← the broken-anchors refusal
```

#### `packages/tddy-index-daemon/src/apply.rs`

```rust
pub(crate) fn apply_plan(root, options, client, cancel, progress, events) -> Result<()> {
    let plan = Plan::parse(&std::fs::read_to_string(options.plan()?)?)?;   // re-read from disk every run
    let paths = StatePaths::under(root);                                   // repo-scoped (code issue)
    let mut journal = runner::open_run_after(&plan, root, &paths, options, || ...)?;
    let mut ledger = runner::restore_ledger(&journal, &paths)?;
    ...
    for (index, op) in plan.ops.iter().enumerate().skip(start) { ... }
```

#### `packages/tddy-index-daemon/src/lib.rs` / `index.rs` / `service.rs`

```rust
//! Two lifetimes, one implementation. With no transport argument the binary runs a single operation
//! ... in process ... With `--grpc` and/or `--stdio` it serves that same implementation and stays alive.
pub struct CodeIndexServiceImpl { index: WorkspaceIndex }
// index.rs: per-root queue, because `.restructure/journal.jsonl` is keyed by root with no lock file
```

#### `packages/tddy-index-daemon/proto/code_index.proto`

```proto
rpc Check(CheckRequest) returns (stream RestructureEvent);
rpc Apply(ApplyRequest) returns (stream RestructureEvent);
rpc Anchors(AnchorsRequest) returns (AnchorsResponse);
rpc PlanStatus(PlanStatusRequest) returns (PlanStatusResponse);
message CheckRequest { ... string plan = 2;  // Path to the JSONL plan, absolute or relative to workspace_root
message AnchorsRequest { string workspace_root = 1; string file = 2; repeated string items = 3; }
message AnchorsResponse { SourceRange range = 1; }
```

#### Code issues

- `tddy-code-restructuring/docs/code-issues/broken-restructure-anchors-empty-outline.md` —
  `restructure anchors` refuses every item; outline empty on warm path, `lsp server exited` cold.
- `tddy-index-daemon/docs/code-issues/stale-repo-scoped-restructure-state-apply.md` — daemon apply
  uses `StatePaths::under(root)`; fix `StatePaths::for_plan(root, options.plan()?)`; Claimed by nobody.
- Others (not in path): `complexity-rust-facade-lines`, `oversized-file-backends-rust`,
  `oversized-file-test-binary`, `complexity-warm-narrate-until-loaded`,
  `poisoned-warm-latch-on-interrupted-index`.

#### Tests

`tddy-code-restructuring/tests/`: 23 suites incl. `snapshot_rewrites_the_header.rs`,
`extract_variable_acceptance.rs`, `extract_method_*_acceptance.rs`, `wedged_request_acceptance.rs`,
shared `harness/`. `tddy-index-daemon/tests/`: `code_index_service_acceptance.rs`,
`warm_index_production.rs`, `detached_daemon_production.rs`, `dual_transport_acceptance.rs`,
`tree_changes_acceptance.rs`, `activity_log_acceptance.rs`, `ping_answers_only_a_live_listener.rs`.

### Findings

1. Two anchor kinds; neither is an LSP path. `Symbol.path` is a bare, first-match name.
2. Range anchors are exact and trusted; staleness is handled only inside one run by the ledger and
   never written back to the JSONL.
3. The daemon has no plan state; it re-parses the file per request and names plans by path.
4. The authoring tool that should emit anchors is broken (empty outline) — anything that resolves an
   LSP path through `documentSymbol` depends on that outline being non-empty.
5. The daemon's apply loop still uses repo-scoped run state.
6. No open-PR collision in these packages.

## Exploration 2: node-specific reads — 2026-09-26

**Agent**: parent Grep/Read (Bash)
**Scope**: the files and records this node's changeset is grounded on, beyond Exploration 1.

### Sequence

1. Read todo `2026-09-25-restructure-move-to-crate-follows-a-facade-back-to-the-destination.md` — What happened / hand fix
2. Read todo `2026-09-25-restructure-move-to-crate-leaves-the-destinations-own-extern-name.md` — `cyclic package dependency`; fix: destination extern name → `crate::`, self-dep an assertion
3. Read todo `2026-09-25-restructure-move-to-crate-misses-a-crate-named-only-in-a-body-path.md` — fix: collect first segment of every path incl. bodies; `#[cfg(test)]` → dev-deps
4. Read todo `2026-09-25-restructure-move-to-crate-reads-an-import-reaching-the-destination-as-an-edge.md` — resolve `self`/`super`/`crate` before stays-behind; follow re-exports; red-test fixture described
5. Read `ls packages/tddy-code-restructuring/src/crate_move` — cluster, destination, header, manifest_edits, module_home, moving, preconditions, refusals, test_binary

### Findings

See this node's changeset `## Technical Changes` → State A; the excerpts above are the evidence.
