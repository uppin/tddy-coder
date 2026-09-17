# Restructure refusal truth and authoring gates - PRD

**Date**: 2026-09-17
**PRD Type**: Bug Fix + Technical Improvement

## Affected Features

- **Primary Feature**: [Rust code restructuring](../rust-code-restructuring.md) — the refusal
  taxonomy, the import-restoration pass, a new extract preflight, a new `snapshot` subcommand, and
  the § Known limitations entries these close.
- **Related Feature**: [Reusable LSP](../reusable-lsp.md) — unchanged in behaviour, but the
  refusals that name rust-analyzer stop being reported as plan defects, which is what a reader of
  an LSP-backed failure acts on.
- **Related surface**: `.agents/skills/code-restructuring/SKILL.md` — the authoring workflow an
  agent follows. Not a `docs/ft/` feature document, but it is the operative instruction set and it
  changes here.

## Summary

`tddy-tools restructure` tells an author their plan is malformed whenever anything goes wrong —
including when the plan is fine and the code, the server, or the transport is the problem. Underneath
that, two real defects make correct plans fail: the import-restoration pass cannot recognise a
re-exported path as the one a file already imports, and nothing refuses an extraction whose leftover
code reaches items the range takes away. The documentation compounds both by presenting the
rehearsal gates that would have caught them as optional.

This PRD splits the refusal taxonomy into classes a caller acts on differently, fixes the two
defects, makes the daemon that removes the six-to-ten-minute retry cost survive the shell that
started it, and rewrites the authoring workflow so the gates are the path rather than a footnote.

## Background

A live `extract_module` spike against the `#carve` node 2 planning seam in
`packages/tddy-workflow-recipes/src/parser.rs` failed twice at roughly twenty minutes of indexing per
attempt, then produced source that did not compile. The handoff written afterwards
(`plans/2026-09-17-restructure-warm-daemon-carve-handoff.md`) catalogued eight failures.

Verifying that catalogue against the tree found that three entries were misdiagnosed, two described
gates that already ship and went unused, and the two real defects were never named. The evidence is
in [the discovery companion](../../dev/1-WIP/2026-09-17-restructure-refusal-truth-and-authoring-gates-initial-discovery.md);
the short version:

- The spike's refusal was `` `ParseError` could be imported 3 ways ``. rust-analyzer offered
  `tddy_core::ParseError` — the re-export at `packages/tddy-core/src/lib.rs:80` — while the file
  imports the canonical `tddy_core::error::ParseError`. `choose_import` compares path **strings**, so
  it could not see that one candidate was the binding the file already had.
- The later `cargo check` failure was not a visibility gap in the direction it appeared to be. A
  child module *can* see its parent's private items — verified by compiled probe — so nothing needed
  widening on the way down. The apply journal shows what really happened: the assist moved only part
  of the anchored range, rewrote the part it left behind to reach into the new module through
  qualified `planning::` paths, and `restore_visibility` — reading a survey taken before the assist
  ran — narrowed exactly those two items back to private.
- Every one of those refusals was reported as `plan is malformed:`. The plan was valid both times.

This last point is already on the record twice. `packages/tddy-index-daemon/src/status.rs:1-8`
exists to stop "the same mistake, one level up, as reporting 'the index did not settle' as 'your
plan is malformed'", and
[`2026-09-09-restructure-defects-from-the-first-cross-crate-move.md`](../../dev/todo/2026-09-09-restructure-defects-from-the-first-cross-crate-move.md)
records `#unbundle` node 6 hitting it. The distinction is enforced where errors reach the wire and
absent where they are constructed.

## Proposed Changes

### What's Changing

**1. `RestructureError` grows two variants, and `failure()` stops being a single constructor.**

`RestructureError::MalformedPlan` currently carries all fifty-six `RustBackend` refusals. They split
into four families that a reader acts on differently:

| Family | Example | What the reader does |
|---|---|---|
| **Malformed plan** | "the operation needs a name"; "`{item}` is not an item `{file}` defines at module level" | Edit the plan |
| **Seam refused** | stranded references; impl siblings; uncovered nesting; split attribute paths; module name taken; import ambiguity | Cut the seam elsewhere, or change the code |
| **Server answer unusable** | "rust-analyzer returned no edits"; inferred `_` placeholder; mangled rewrite; foreign position encoding | Retry warm, or look at the server |
| **Transport** | "rust-analyzer is not running"; "closed the connection"; "unreadable Content-Length" | Already served by `Io` / `ServerCatchingUp` |

Two new variants — `SeamRefused(String)` and `ServerDefect(String)` — with the first two families
routed to them and the rest left on `MalformedPlan`. The refusal *messages* do not change: they
already end with their remedy. What changes is the sentence in front of them, from "plan is
malformed:" to "this seam cannot be cut here:" and "rust-analyzer's answer was unusable:".

`status_of` in `tddy-index-daemon` classifies the new variants — `SeamRefused` as
`FailedPrecondition` (the request is well formed; the tree is not in a state that permits it) and
`ServerDefect` as `Internal`. The exhaustive match means omitting either is a compile error.

**2. `choose_import` learns that two paths can name one item.**

A third disambiguation tier, tried only after the existing exact-path and parent-module tiers both
decline: among the offered candidates, prefer the one whose **crate root** matches the crate root of
an in-scope binding of that same name. For the spike's case that is `tddy_core` against
`tddy_core::error::ParseError` — decisive over `std::string::ParseError` and `chrono::ParseError`.
Two candidates rooted in crates the file binds that name from is still the ambiguity the function
exists to refuse.

Strictly weaker evidence than an exact match, so strictly lower precedence. The existing verify loop
is unchanged: whatever this tier picks is still tried and still rejected if it fails to reduce the
name's unresolved occurrences.

**3. `restore_visibility` decides against the text the assist produced, not the pre-assist survey.**

The spike's `E0603` is proven, not inferred. `it3z/.restructure/journal.jsonl` holds rust-analyzer's
exact edit: the anchor asked for lines 10–152, the assist relocated **only 10–116**, and it rewrote
the leftover in place to reach into the new module —

```rust
let parsed: planning::StructuredPlan = serde_json::from_str(s)
} else if planning::prd_value_looks_like_md_file_path(&prd) {
```

— while the module it wrote declares both of those privately:

```rust
struct StructuredPlan {
fn prd_value_looks_like_md_file_path(prd: &str) -> bool {
```

`survey_moved_items` runs on the **original** text over the **requested** range, so the only
references to those two items were inside the range and `reached_from_outside` came back `false`.
`restore_visibility` then narrowed both back to private — correctly, by its own evidence, and wrongly
in fact, because the assist had just created outside references by declining to move their caller.

The fix is the principle the crate already applies one line below, for `impl` members: *"Read off the
text the assist actually produced, so a member it left alone is not reported as though it had
moved."* That reading currently feeds only the **report**. It must also feed the **decision**: an
item the produced parent still reaches through `module::Item` keeps its widening, whatever the
pre-assist survey concluded.

Paired with a guard, because a partial relocation is worth knowing about on its own: when the assist
moves materially less than the anchor asked for, the run says so. That is a `SeamRefused` naming the
lines left behind, and `anchors --items` is the remedy it points at.

**4. `restructure snapshot <plan.jsonl>` rewrites the header from the working tree.**

Every edit to a snapshotted file invalidates the plan's `sha256:` header, and recomputing it is
currently a hand-rolled shell pipeline the author has to invent. `hash_file` is already public; the
subcommand reads the header, re-hashes each named path, and writes the line back. It takes no LSP
client and needs no index.

**5. `run-index-daemon` detaches, and `--status` dials.**

The daemon is started with `nohup … &` from the script's own shell — protected from `SIGHUP`, but
still in the caller's process group, so a harness that tears that group down on command completion
takes the daemon with it. It moves into its own session. `--status` stops reporting a live daemon on
the strength of `kill -0` and instead opens a gRPC connection and issues `Workspaces`, the cheapest
unary call on the service. A socket that will not answer is reported as not answering — there is no
fall back to the pid check.

**6. `apply` gains the verdict step `check` and `verify` already have.**

`check` ends on `verdict_on_findings()` and `verify` on its comparison; `apply` returns `Ok(())`
after draining its event stream, so its exit status depends entirely on the stream erroring. A
refused op does error the stream today, so this is a structural gap rather than a live bug — but it
is the gap that would make a partial apply look like a clean one, and it costs one function to
close.

**7. The authoring workflow states the gates as gates.**

In `SKILL.md` and `docs/ft/coder/rust-code-restructuring.md`:

- `check --deep` is the seam-proving step, not a bracketed option, with one line saying why: a plain
  `check` reads text and cannot see a refusal from the assist path.
- `anchors --items` is how a range is authored. Hand-written line numbers are the reason the spike's
  anchor split a dependency cluster.
- `snapshot` is named where the workflow says to snapshot.
- A short § on what the import pass can and cannot restore, including the re-export case this PRD
  fixes and the aliased-import case D8 already fixed.

### What's Staying the Same

- **Plans still hold intents only.** No new operation, no change to the JSONL schema, no code text
  in plans.
- **No fallbacks.** Each refusal stays a refusal; the new classes change what a refusal is *called*,
  never whether it happens.
- **`restore_visibility` and `impl_widenings` are untouched.** They are correct; the "widen what was
  left behind" idea they appeared to need is a misreading of Rust's visibility rules.
- **The seven operations, five subcommands surface** gains `snapshot` and nothing else.
- **`TDDY_INDEX_SOCKET` semantics** — set and unreachable stays an error, unset stays the cold path.
- **Every existing refusal message's wording**, so the remedies already written stay written.

## Impact Analysis

### Technical Impact

| Package | Change |
|---|---|
| `tddy-code-restructuring` | Two error variants; ~56 `failure()` call sites re-routed through three constructors; `choose_import` third tier; one new preflight; `snapshot` subcommand and its `Options`/`Command` wiring |
| `tddy-index-daemon` | `status_of` arms for the two new variants |
| `tddy-tools` | `apply` verdict step in `index_client` |
| repo root | `run-index-daemon` detach + dialling `--status` |
| `.agents/skills/code-restructuring`, `docs/ft/coder` | The authoring gates |

**Performance**: the new preflight reads text and runs before the server is started, so a refusal it
raises costs nothing instead of a full index. `choose_import`'s third tier is a string comparison
over paths already in hand. `--status` adds one unary RPC to a command that currently does no I/O.

**Dependencies**: none added.

**Integration points**: `status_of`'s exhaustive match is the compile-time guarantee that the daemon
keeps classifying every variant. The `#carve` stack (#488–#498) consumes this tooling; only #488
touches this crate, and it touches `crate_move.rs` rather than the import pass or the enum.

**Size**: every Rust change lands in `backends/rust.rs`, already recorded at 4,571 production lines
and over budget. The net addition is held to roughly one preflight plus one chooser tier, and the
helper tail is left where the carve entry expects to find it.

### User Impact

The audience is an agent or developer authoring a restructuring plan.

- A refusal now says which of three things to go and fix. Today all of them say "fix your plan".
- Two classes of correct plan that fail today will succeed: one blocked by a re-exported import path,
  one that currently succeeds into non-compiling source.
- A warm daemon survives the shell that started it, so the twenty-minute retry is paid once.
- The workflow an author follows produces a correct anchor and rehearses before writing.

**Breaking changes**: none at the CLI. Exit codes, output formats and the plan schema are unchanged.
Callers matching on the gRPC status of a seam refusal see `FailedPrecondition` where they saw
`InvalidArgument` — the point of the change, and no in-repo caller matches on it.

**Migration**: none.

## Implementation Plan

1. **Refusal taxonomy** — add `SeamRefused` and `ServerDefect`; add `seam_refusal()` and
   `server_defect()` beside `failure()`; re-route the call sites family by family; extend
   `status_of`. Tests pin the class of each refusal constructor and the gRPC status of each.
2. **Import disambiguation** — third tier in `choose_import`, with unit tests over the spike's exact
   candidate list and over the ambiguous case that must still refuse.
3. **Leftover-reference preflight** — survey and refusal, raised before the server starts, with the
   parser seam's real shape as the fixture.
4. **`snapshot` subcommand** — args, runner command, and a test that a stale header is rewritten to
   match the tree and that an unchanged plan is a no-op.
5. **Daemon operability** — detach into a new session; `--status` dials `Workspaces`.
6. **Authoring gates** — skill and feature doc, including the § on the import pass, and the
   § Known limitations entries these close.

Landing as one PR off `master`. Not a stack: items 1 and 3 share the refusal constructors, item 6
documents items 2–4, and splitting them would produce nodes that cannot be reviewed apart.

## Acceptance Criteria

- [ ] A seam refusal reports as a seam refusal, not as a malformed plan ([Rust code restructuring](../rust-code-restructuring.md))
- [ ] A defect in rust-analyzer's answer reports as a server defect, not as a malformed plan
- [ ] A genuinely malformed plan still reports as a malformed plan
- [ ] `status_of` maps a seam refusal to `FailedPrecondition` and a server defect to `Internal`
- [ ] A name offered under a re-exported path is imported when the file binds it canonically from the same crate
- [ ] A name whose candidates are rooted in two crates the file binds it from is still refused
- [ ] An item the produced parent reaches through `module::Item` keeps its widening, although the pre-assist survey saw no outside reference
- [ ] An item nothing outside reaches after the assist is still narrowed back to the visibility it was written with
- [ ] An assist that relocates materially less than the anchor asked for is refused, naming the lines left behind
- [ ] `restructure snapshot` rewrites a stale header to match the working tree
- [ ] `run-index-daemon` leaves a daemon running after the shell that started it exits
- [ ] `run-index-daemon --status` reports a socket that will not answer as not answering
- [ ] `apply` carries a verdict step of its own
- [ ] The skill and the feature doc state `check --deep`, `anchors --items` and `snapshot` as the workflow

## References

### Affected Features (Complete List)

- [Rust code restructuring](../rust-code-restructuring.md) — refusal classes, import restoration, the
  new preflight, the `snapshot` subcommand, § Known limitations
- [Reusable LSP](../reusable-lsp.md) — no behaviour change; refusals naming rust-analyzer are no
  longer classed as plan defects

### Related Documentation

- [Initial discovery](../../dev/1-WIP/2026-09-17-restructure-refusal-truth-and-authoring-gates-initial-discovery.md)
- [2026-09-09-restructure-defects-from-the-first-cross-crate-move.md](../../dev/todo/2026-09-09-restructure-defects-from-the-first-cross-crate-move.md) — `#unbundle` node 6 on the wrong error class
- [2026-09-09-restructure-defects-from-the-connection-service-split.md](../../dev/todo/2026-09-09-restructure-defects-from-the-connection-service-split.md) — D8, the same shape as this PRD's import fix
- [2026-09-16-backends-rust-rs-is-4500-production-lines.md](../../dev/todo/2026-09-16-backends-rust-rs-is-4500-production-lines.md) — the file every change here lands in
- `plans/2026-09-17-restructure-warm-daemon-carve-handoff.md` — the spike this corrects (worktree-local; `/plans/` is gitignored)
