# Initial Discovery: Restructure refusal truth and authoring gates

**Changeset**: [2026-09-17-restructure-refusal-truth-and-authoring-gates.md](./2026-09-17-restructure-refusal-truth-and-authoring-gates.md)
**Date**: 2026-09-17
**Passes**: 3

## Combined Conclusions

The trigger for this change is a handoff document written after a live `extract_module` spike
against the `#carve` node 2 planning seam in `packages/tddy-workflow-recipes/src/parser.rs`. The
spike failed twice, cost roughly twenty minutes of indexing per failure, and produced a failure
catalogue with eight entries. Verifying that catalogue against the tree established that **three of
its eight entries are misdiagnosed, two of them describe gates that already ship, and the two real
defects underneath were never named**. This changeset fixes the real defects and closes the gap
between what the tooling can do and what its documentation tells an author to do.

### What is actually broken

**1. `RestructureError::MalformedPlan` is a bucket carrying fifty-six unrelated refusals.**
`failure()` (`backends/rust.rs:4568`) maps every `RustBackend` refusal to `MalformedPlan`, whose
`Display` is `"plan is malformed: {0}"`. The call sites fall into four families that a caller acts
on differently — a genuinely malformed plan, a seam the code will not permit, a defect in what
rust-analyzer answered, and a transport failure — and all four tell the operator to fix their plan.
`tddy-index-daemon/src/status.rs` then maps `MalformedPlan` to gRPC `InvalidArgument`, so the wrong
class propagates to every transport. The irony is on the record: `status.rs:1-8` exists precisely to
stop "the same mistake, one level up, as reporting 'the index did not settle' as 'your plan is
malformed'", and `docs/dev/todo/2026-09-09-restructure-defects-from-the-first-cross-crate-move.md`
records the same complaint from `#unbundle` node 6. The distinction is enforced at the transport
boundary and absent at the source.

**2. `choose_import` compares import paths by string equality, so a re-export defeats it.** This is
the actual cause of the spike's `ParseError` refusal, and it is not what the handoff says. From
`/tmp/restructure-apply-visible.log:397`, rust-analyzer offered `tddy_core::ParseError` — the
*re-exported root path*, because `packages/tddy-core/src/lib.rs:80` is
`pub use error::{BackendError, ParseError, WorkflowError};`. The file's own import is the *canonical*
path, `use tddy_core::error::ParseError;` (`parser.rs:7`). `choose_import`
(`backends/rust.rs:2829-2839`) tests `in_scope.contains(&path)` on the full path string, so the two
never match; the `parent_module` fallback (`:2848-2859`) then compares `tddy_core` against
`tddy_core::error` and misses too. The refusal fires with three candidates —
`tddy_core::ParseError`, `std::string::ParseError`, `chrono::ParseError` — although only one of them
shares a crate with a binding the file already has. This is the same shape as **D8** in
`docs/dev/todo/2026-09-09-restructure-defects-from-the-connection-service-split.md`, where
rust-analyzer offered the unaliased path for an aliased import; that entry's closing note — "the
remaining obstacles are all in the import-restoration pass" — predicted this one.

**3. `restore_visibility` decides on a survey the assist has already invalidated.** Proven from the
apply journal in Exploration 3, not inferred. The anchor asked for lines 10–152; the assist
relocated **only 10–116** and rewrote the part it left behind to reach into the new module through
qualified paths (`planning::StructuredPlan`, `planning::prd_value_looks_like_md_file_path`). But
`survey_moved_items` (`backends/rust.rs:1424`) had run earlier, on the *original* text over the
*requested* range, where those references were inside the range — so `reached_from_outside` was
`false` and `restore_visibility` narrowed both items back to private. `E0603` at the next build,
after `applied 1 of 1 operations`. One cause, not the three the handoff lists.

The corrective principle is already in the file, one line below, for `impl` members: *"Read off the
text the assist actually produced, so a member it left alone is not reported as though it had
moved."* It feeds the **report** and not the **decision**.

### What is not broken, and must not be "fixed"

- **A child module sees its parent's private items.** Verified by compiling a probe
  (Exploration 2, step 7): a private parent struct, its private fields, and a private parent fn are
  all reachable from a child module. There is therefore no "widen what was left behind" work in the
  downward direction, and the `pub(crate)` the spike hand-wrote onto `StructuredPlan`'s fields is
  noise rather than repair. `restore_visibility`'s *narrowing rule* is right; only the evidence it
  narrows on is stale.
- **A preflight cannot catch defect 3.** The leftover references do not exist until the assist has
  run and rewritten the parent, so nothing before the server could see them. This ruled out the
  approach the PRD originally carried.
- **Refused runs already exit non-zero.** Both front ends return `Err` and `tddy-tools::main` is
  `-> Result<()>` with `?`. The handoff's "exit 0 despite Error:" is an artifact of the spike's own
  shell wrapper: `/tmp/restructure-apply-warm.log` prints `=== exit 0 ===` after a *connection
  refused*, and `/tmp/restructure-apply-warm2.log` prints `=== exit  ===` — an empty status. The
  wrapper was reading `tee`'s exit code.
- **`check --deep` already rehearses the apply.** `runner/rehearsal.rs:48-70` calls the same
  `RustBackend::resolve` that `apply` calls, and writes nothing. Both refusals the spike paid a
  full apply to discover were available from it. `apply --dry-run` is a second rehearsal.
- **`restructure anchors --items` already emits a correct range** covering named items including
  trivia (`restructure_args.rs:67-73`). The spike hand-wrote `line 10 → line 152` instead.

### What the documentation gets wrong

`.agents/skills/code-restructuring/SKILL.md` step 7 offers `--deep` as a bracketed option and step 8
says "`--dry-run` then apply" without saying why either matters. Neither the skill nor
`docs/ft/coder/rust-code-restructuring.md` mentions that a plain `check` cannot see an assist-path
refusal, that `anchors --items` is how a range should be authored, or that a snapshot hash has to be
recomputed by hand after every edit to a snapshotted file. An author following the skill exactly
would have made every choice the spike made.

### Packages in play

| Package | Why |
|---|---|
| `packages/tddy-code-restructuring` | `RestructureError`, `failure()`, `choose_import`, the refusal constructors, the CLI front end and its subcommands |
| `packages/tddy-index-daemon` | `status_of` must classify the new variants; exhaustive match makes this a compile error rather than a silent fold |
| `packages/tddy-tools` | `index_client` renders warm-path refusals and owns `apply`'s missing verdict step |
| repo root `run-index-daemon` | detaching the daemon, and a `--status` that dials rather than signals |
| `.agents/skills/code-restructuring`, `docs/ft/coder` | the authoring gates |

### Constraints

- `backends/rust.rs` is 4,571 production lines and is already recorded as over budget
  (`docs/dev/todo/2026-09-16-backends-rust-rs-is-4500-production-lines.md`). Every change here lands
  in it. That entry asks for a carve of the helper tail — which is where `choose_import` lives — so
  this change must not grow that tail materially, and must not pre-empt the carve either.
- The refusal messages are unusually good: each one already ends with the remedy. The taxonomy work
  is about the *class* they are reported under, not a rewrite of their text.
- No fallbacks (`CLAUDE.md`). A daemon whose `--status` cannot dial reports that it cannot dial; it
  does not fall back to a pid check and call the daemon healthy.

### Conflicts with other WIP changesets

`docs/dev/1-WIP/` holds three changesets — `2026-08-31-split-sandbox-orchestration`,
`2026-08-31-split-sandbox-resume`, `2026-09-09-livekit-rooms-panel-every-connection`. None touches
`tddy-code-restructuring`, `tddy-index-daemon` or the restructuring skill. No conflict.

The ten open `#carve` PRs (#488–#498) are consumers of this tooling, not co-editors of it: only #488
(`feature/carve/restructure-moves`) touches `tddy-code-restructuring`, and it touches
`crate_move.rs` and the `check` preflight rather than the import pass or the error enum.

### Open questions

- Whether the new variants should be three (`SeamRefused`, `ServerDefect`, keep `MalformedPlan`) or
  four (splitting transport out of `ServerDefect`). Leaning three: the transport failures already
  have `Io` and `ServerCatchingUp` beside them, and a fourth class nobody routes differently is
  ceremony.
- Whether the crate-root disambiguation rule should also apply to the `parent_module` fallback, or
  replace it. Leaning "additional tier, tried after the existing two" — it is strictly weaker
  evidence than an exact path match and must not outrank it.

---

## Exploration 1: Verifying the handoff document against the tree — 2026-09-17

**Agent**: parent Grep/Glob/Read (Bash)
**Scope**: every factual claim in `plans/2026-09-17-restructure-warm-daemon-carve-handoff.md` — the
merged PRs, the branch and stack state, the four subcommands, the import-restoration path, the exit
codes, the daemon script, and the spike's own artifacts.

### Sequence

1. `git log --oneline -8` and `git branch -a --list '*carve*' '*restructure*'` — confirm #500/#501
   landed and find the carve branches.
2. `ls plans/`, `ls docs/dev/todo/` — locate the referenced prior analysis and the todo entries the
   handoff cites.
3. `ls packages/tddy-workflow-recipes/src/`, `grep -n "ParseError" parser.rs` — establish the seam's
   current shape.
4. `grep -rn "could be imported" --include='*.rs' packages/` — find the refusal the spike hit.
5. Read `backends/rust.rs:1700-1900` (`prune_assist_imports`, `next_import`) — the import-restoration
   pass.
6. Read `backends/rust.rs:2824-2865` (`choose_import`, `parent_module`) and `:4245-4300`
   (`alias_target`, `parent_binding`) — how a candidate is chosen.
7. `grep -rn "fn failure" packages/tddy-code-restructuring/src/` and `grep -rn "plan is malformed"` —
   how the refusal is classed.
8. Read `restructure_args.rs:1-120` — the actual subcommand surface.
9. `grep -rn "deep" packages/tddy-code-restructuring/src/**/*.rs`, read `runner/rehearsal.rs:1-110` —
   what `--deep` does.
10. Read `backends/rust.rs:1235-1300` (`resolve`) and `:1403-1490` (`assisted_edit`) — confirm the
    rehearsal and the apply share a path.
11. Read `index_client.rs:1-170`, `index_console.rs:58-160`, `operations.rs:144-196` — the warm-path
    exit behaviour.
12. Read `run-index-daemon:50-200` — the daemon lifecycle and `--status`.
13. `gh pr view 488`, `gh pr list --search carve` — the live stack.
14. `git log origin/master..origin/feature/carve/recipe-parsers`, `git show --stat 23732000` — the
    node's red phase.
15. `ls`/`cat` the spike artifacts in `/Users/mantasi/.cursor/worktrees/tddy-coder/it3z` and `/tmp`.

### Grep / glob

| Tool | Pattern / glob | Path scope | Notable hits |
|------|----------------|------------|--------------|
| Grep | `could be imported` | `packages/` (`*.rs`) | `backends/rust.rs:1823` — the only producer |
| Grep | `fn failure` | `packages/tddy-code-restructuring/src/` | `backends/rust.rs:4568` |
| Grep | `plan is malformed` | `packages/tddy-code-restructuring/src/` | `lib.rs:35` — the `MalformedPlan` `Display` |
| Grep | `deep` | `packages/tddy-code-restructuring/src/**` | `runner/rehearsal.rs:3`, `restructure_cli.rs:266`, `runner/entry_points.rs:281` |
| Grep | `fn choose_import\|fn parent_binding\|fn already_bound\|fn alias_target` | `backends/rust.rs` | `2948`, `2987`, `4253`, `4274` |
| Grep | `InvalidArgument` | `packages/tddy-index-daemon/src/` | `status.rs:17`, `status.rs:137` |
| Grep | `status\|nohup\|kill -0\|grpc` | `run-index-daemon` | `:59`, `:105`, `:173`, `:185` |
| Grep | `rpc \|service ` | `packages/tddy-index-daemon/proto/code_index.proto` | `Warm`, `Check`, `Apply`, `Anchors`, `PlanStatus`, `Verify`, `Workspaces` |
| Bash | `git ls-tree origin/feature/carve/recipe-parsers packages/tddy-workflow-recipes/src/` | — | no `parser/` directory; branch still on the pre-#500 chain |

### Inspected files

#### `packages/tddy-code-restructuring/src/lib.rs`

**Why**: establish what error classes exist and what `MalformedPlan` means.
**Excerpt**:

```rust
#[error("plan is malformed: {0}")]
MalformedPlan(String),
…
/// Kept apart from [`RestructureError::MalformedPlan`] because a caller acts on the difference:
/// a malformed plan is fixed by editing the plan, and a server that will not settle is fixed by
/// waiting or by looking at the server. Reporting the second as the first is what makes the
/// advice "fix your plan" actively misleading.
ServerNotSettled { method: String, seconds: u64, last: String },
```

The argument for a separate variant is already written down — and applied exactly once.

#### `packages/tddy-code-restructuring/src/backends/rust.rs`

**Why**: the refusal path, the import chooser, and the error constructor.
**Excerpt** (`:4568`, the constructor every refusal goes through):

```rust
fn failure(reason: impl Into<String>) -> RestructureError {
    RestructureError::MalformedPlan(reason.into())
}
```

**Excerpt** (`:2824-2859`, the chooser — note `in_scope.contains(&path)` is full-string):

```rust
fn choose_import<'a>(text: &str, offered: &[&'a str]) -> Option<&'a str> {
    if let [only] = offered { return Some(only); }
    let in_scope = imported_paths(text);
    let mut narrowed = offered.iter()
        .filter(|title| import_path(title).is_some_and(|path| in_scope.contains(&path)));
    if let Some(only) = narrowed.next() {
        if narrowed.next().is_none() { return Some(only); }
        return None;
    }
    let modules: Vec<String> = in_scope.iter().filter_map(|path| parent_module(path)).collect();
    let mut by_module = offered.iter().filter(|title| {
        import_path(title).and_then(|path| parent_module(&path))
            .is_some_and(|module| modules.contains(&module))
    });
    let only = *by_module.next()?;
    by_module.next().is_none().then_some(only)
}
```

**Excerpt** (`:1814-1827`, the refusal the spike hit):

```rust
let ordered = import_order(text, &offered).ok_or_else(|| {
    failure(format!(
        "`{}` could be imported {} ways and neither rust-analyzer nor this file's own \
         imports say which the moved code meant: {}",
        name.text, offered.len(), offered.join(", ")
    ))
})?;
```

**Excerpt** (`:3658`, the guard that exists for `impl` members only):

```rust
fn refuse_impl_sibling_references(items: &[MovedItem]) -> Result<()> {
```

#### `packages/tddy-code-restructuring/src/runner/rehearsal.rs`

**Why**: decide whether `check --deep` really predicts an apply.
**Excerpt**:

```rust
//! What `check --deep` does beyond reading text: resolve the operation to learn the refusal an
//! apply would give, and survey a cross-crate move to learn its blast radius. Writes nothing.
…
let resolved = registry
    .backend_for(Path::new(at.anchor.file()), at.op)?
    .resolve(&at, &Workspace { root, overlay: &self.overlay });
```

Same `resolve` the apply calls. The handoff's F7 asks for a mode that already exists.

#### `run-index-daemon`

**Why**: the daemon that "dies after binding", and what `--status` proves.
**Excerpt** (`:105-113` — a pid check, not a dial):

```bash
  --status)
    if pid="$(running_pid)"; then
      narrate "Index daemon pid ${pid} is serving ${SOCKET}"
      echo "export TDDY_INDEX_SOCKET=${SOCKET}"
```

**Excerpt** (`:173` — `nohup` and `&`, but no new session):

```bash
PATH="$DEV_SHELL_PATH" RUST_LOG="${TDDY_INDEX_LOG:-info}" nohup "$BINARY" --grpc-uds "$SOCKET" \
  </dev/null >/dev/null 2>"$LOG_FILE" &
```

#### `packages/tddy-tools/src/index_client.rs`

**Why**: the warm path's verdict behaviour.
**Excerpt** (`check` ends on a verdict; `apply` does not):

```rust
    let mut events = client.check(request).await.map_err(refused)?.into_inner();
    …
    rendered.verdict_on_findings()
}
…
    let mut events = client.apply(request).await.map_err(refused)?.into_inner();
    let mut rendered = Rendered::new(rehearsal);
    while let Some(event) = events.message().await.map_err(refused)? {
        rendered.event(&event)?;
    }
    Ok(())
```

### Findings

- #500 (`35a157c0`) and #501 (`8b98c5db`) are merged. #488 is open, draft, based on `master`.
- The `#carve` stack is **ten** nodes, not nine: PR titles read `(#carve N/10)` while the commit
  subjects on the branches still read `N/9`.
- `origin/feature/carve/recipe-parsers` is stale — still on the pre-#500 chain — and carries a red
  phase (`23732000`, `tests/module_shape.rs`) pinning **six** parser modules and **four** hook
  halves. The spike delivered one of six.
- The spike's uncommitted result lives in `it3z` on `feature/restructure-apply-progress`, whose PR
  is merged.
- `check --deep`, `apply --dry-run` and `anchors --items` all ship and were all unused.
- Both front ends exit non-zero on a refusal; the spike's wrapper was reading `tee`'s status.

---

## Exploration 2: Scoping the fixes — 2026-09-17

**Agent**: parent Grep/Read/Bash, including one compiled probe
**Scope**: the four work areas — refusal taxonomy, import disambiguation, the leftover-reference
guard, daemon operability — plus the snapshot helper and the documentation gates.

### Sequence

1. Read `lib.rs:32-110` — the complete `RestructureError` enum.
2. `grep -n "failure(" backends/rust.rs` — enumerate and classify all fifty-six call sites.
3. Read the message bodies at the sixteen `format!` call sites — establish the four families.
4. `grep -n "^fn refuse_" backends/rust.rs` — the ten refusal constructors.
5. Read `backends/rust.rs:3565-3585` (`MovedItem`), `:4351-4400` (`restore_visibility`),
   `:3797-3820` (`impl_widenings`) — the visibility machinery.
6. Read `/tmp/restructure-apply-visible.log`, `/tmp/restructure-apply-warm*.log`,
   `/tmp/restructure-check-warm2.log` — the spike's real failure output.
7. **Compiled probe**: `rustc --edition 2021 scratchpad/vis.rs` in the dev shell — does a child
   module see a private parent struct, its private fields, and a private parent fn?
8. `grep -n "pub use error" packages/tddy-core/src/lib.rs` — is `ParseError` re-exported at the root?
9. Read `plan.rs:150-210` and `apply.rs:56-72` — snapshot header parsing and `hash_file`.
10. Read `packages/tddy-index-daemon/src/status.rs:1-70` — the error-to-`Status` mapping.
11. `grep -rn "rpc " proto/code_index.proto` — find a cheap unary call for a health dial.
12. Read `.agents/skills/code-restructuring/SKILL.md` and
    `docs/ft/coder/rust-code-restructuring.md:41-60,143-160` — what the author is told.
13. Read the four candidate `docs/dev/todo/` entries in full (Step 2b).

### Grep / glob

| Tool | Pattern / glob | Path scope | Notable hits |
|------|----------------|------------|--------------|
| Grep | `failure(` | `backends/rust.rs` | 56 call sites; 16 carry a `format!` message |
| Grep | `^fn refuse_` | `backends/rust.rs` | `2323`, `2371`, `3333`, `3369`, `3597`, `3658`, `3844`, `3968`, `4021`, `4132` |
| Grep | `ParseError\|could be imported` | `/tmp/restructure-apply-visible.log` | `:397` — the three candidates |
| Grep | `pub use\|pub mod error` | `packages/tddy-core/src/lib.rs` | `:14 pub mod error;`, `:80 pub use error::{BackendError, ParseError, WorkflowError};` |
| Grep | `Sha256\|fn hash_file` | `packages/tddy-code-restructuring/src/` | `apply.rs:9`, `apply.rs:66-72` |
| Grep | `InvalidArgument` | `packages/tddy-index-daemon/src/status.rs` | `:17`, `:26-30` |
| Bash | `rustc --edition 2021 vis.rs` | scratchpad | compiled clean — no visibility error |

### Inspected files

#### `packages/tddy-core/src/lib.rs`

**Why**: explain why rust-analyzer offered `tddy_core::ParseError` rather than the canonical path.
**Excerpt**:

```rust
pub mod error;               // :14
…
pub use error::{BackendError, ParseError, WorkflowError};   // :80
```

Two paths name one item. `parser.rs:7` imports the canonical one; rust-analyzer offers the shortest
one. `choose_import` compares strings, so they do not match.

#### `/tmp/restructure-apply-visible.log`

**Why**: the actual refusal, rather than the handoff's paraphrase.
**Excerpt** (`:397`):

```text
Error: plan is malformed: `ParseError` could be imported 3 ways and neither rust-analyzer nor this
file's own imports say which the moved code meant: Import `tddy_core::ParseError`, Import
`std::string::ParseError`, Import `chrono::ParseError`
=== exit 0 2026-09-15T23:15:01+03:00 ===
```

Two findings in four lines: the offered path is the re-export, and the `exit 0` is the wrapper's
(`/tmp/restructure-apply-warm.log` prints the same `exit 0` after a *connection refused*, and
`warm2` prints an empty status).

#### `scratchpad/vis.rs` (compiled probe)

**Why**: test the handoff's claim that moved code needs parent items widened.
**Excerpt**:

```rust
struct StructuredPlan { goal: Option<String> }
fn prd_value_looks_like_md_file_path(p: &str) -> bool { p.ends_with(".md") }

mod planning {
    use super::{prd_value_looks_like_md_file_path, StructuredPlan};
    pub fn parse(s: &str) -> bool {
        let p = StructuredPlan { goal: Some(s.to_string()) };
        p.goal.is_some() && prd_value_looks_like_md_file_path(s)
    }
}
```

Compiles clean. A child sees an ancestor's private items, fields included. The hazard is the
**reverse** direction, which `pub use child::*;` cannot cover.

#### `packages/tddy-index-daemon/src/status.rs`

**Why**: where the class reaches the wire, and what has to change with the enum.
**Excerpt**:

```rust
//! One function per error type, matched exhaustively, for two reasons. … And a variant added to
//! either error enum later has to be a compile error here rather than silently folding into
//! `Internal` — the same mistake, one level up, as reporting "the index did not settle" as
//! "your plan is malformed".
…
RestructureError::MalformedPlan(_)
| RestructureError::CodeTextInPlan { .. }
| RestructureError::UnsupportedOp { .. }
| RestructureError::NoBackend { .. } => Status::invalid_argument(refusal),
```

The exhaustive match is the safety net: new variants cannot be forgotten here.

#### `packages/tddy-code-restructuring/src/apply.rs`

**Why**: the snapshot helper needs the hash function the runner already uses.
**Excerpt**:

```rust
pub fn hash_file(path: &Path) -> Result<String> {
    …
    Ok(format!("sha256:{:x}", Sha256::digest(&contents)))
}
```

Already `pub`. A `snapshot` subcommand is a rewrite of the header line using this, not new hashing.

#### `.agents/skills/code-restructuring/SKILL.md`

**Why**: what an author is actually told to do.
**Excerpt**:

```markdown
7. **Prove seams** — `tddy-tools restructure check plan.jsonl [--deep]` before apply.
8. **Apply** — `--dry-run` then apply; `verify --against HEAD` after.
```

`--deep` reads as garnish, and nothing says a plain `check` cannot see an assist refusal. Step 5
says "**Snapshot** — `sha256:` hashes in plan header line 1" without saying how to produce them.

### Findings

- The fifty-six `failure()` call sites resolve into four families: **plan** ("the operation needs a
  name", "`--items` named nothing to cover", "`{item}` is not an item `{file}` defines at module
  level"), **seam** (stranded references, impl siblings, uncovered nesting, split attributes, module
  name taken, non-adjacent items, import ambiguity), **server answer** ("rust-analyzer returned no
  edits", "did not produce a `{declaration}`", inferred/residual placeholder, mangled rewrite,
  foreign encoding), and **transport** ("rust-analyzer is not running", "closed the connection",
  "unreadable Content-Length").
- Every seam message already ends with its remedy. Only the class is wrong.
- `Workspaces` is a unary RPC with a trivial request — the right dial for a `--status` health check.
- The snapshot helper needs no new hashing; `hash_file` is public.
- `restore_visibility` and `impl_widenings` are correct and out of scope.

### TODO backlog scan (Step 2b)

Scanned with
`grep -rl -iE '(restructure|index.daemon|rust-analyzer|extract_module|choose_import|malformed)' docs/dev/todo/`
— twenty hits, four of them substantive. Bodies read in full; verdicts recorded in the changeset's
`## Prerequisites`.

---

## Exploration 3: Proving the `E0603` from the apply journal — 2026-09-17

**Agent**: parent Read/Bash over the spike worktree
**Scope**: the one claim I had twice corrected and still could not source — what actually made
`cargo check -p tddy-workflow-recipes` fail after the successful apply. Planning a fix on a third
unverified premise was not acceptable, so this pass went looking for the raw post-apply state rather
than the handoff's description of it.

### Sequence

1. `ls /Users/mantasi/.cursor/worktrees/tddy-coder/it3z/.restructure/` — is the apply's journal still
   on disk? It is: `journal.jsonl` (7,275 bytes) and `ledger.json`, written 06:37.
2. Parse `journal.jsonl` and list every change the completed record carries — which files, which
   ranges, how much text.
3. Print the parent file's edits verbatim — what the assist left behind and how it rewrote it.
4. Print every declaration in the created child, with its visibility.
5. Re-read `backends/rust.rs:1424` (`survey_moved_items` call) and `:1465-1472` against the result.

### Grep / glob

| Tool | Pattern / glob | Path scope | Notable hits |
|------|----------------|------------|--------------|
| Bash | `python3` over `.restructure/journal.jsonl` | `it3z` | 1 completed record, 2 changed files, 12 edits |
| Bash | regex `^\s*(pub \|pub\(crate\) \|)?(struct\|fn\|enum) ` over the child's `newText` | — | `struct StructuredPlan`, `fn prd_value_looks_like_md_file_path` — both private |

### Inspected files

#### `it3z/.restructure/journal.jsonl`

**Why**: the only surviving record of what rust-analyzer produced, as distinct from the
hand-normalized tree `git diff` shows.
**Excerpt** — the parent's edits, verbatim:

```text
--- line 10 -> 27    'mod planning;\npub use planning::*;\n'
--- line 28 -> 116   ''
--- line 119 -> 120  ') -> Result<planning::PlanningOutput, tddy_core::error::ParseError> {\n'
--- line 121 -> 122  '    let parsed: planning::StructuredPlan = serde_json::from_str(s)\n'
--- line 140 -> 141  '        } else if planning::prd_value_looks_like_md_file_path(&prd) {\n'
--- line 148 -> 149  '    Ok(planning::PlanningOutput {\n'
```

**Excerpt** — the declarations in the child the same edit created:

```rust
use super::parse_planning_response_impl;
pub struct PlanningOutput {
pub enum DemoMode {
pub struct PortMap {
pub struct DemoPlan {
pub struct DemoStep {
struct StructuredPlan {                                    // private
pub fn parse_planning_response(
pub fn parse_planning_response_with_base(
fn prd_value_looks_like_md_file_path(prd: &str) -> bool {  // private
```

#### `packages/tddy-code-restructuring/src/backends/rust.rs`

**Why**: locate the disagreement the journal exposes.
**Excerpt** (`:1424` — the survey reads the *original* text over the *requested* range):

```rust
let moved = if relocates {
    self.survey_moved_items(uri, original, range)?
```

**Excerpt** (`:1465-1472` — the principle, applied to the report only):

```rust
let pruned = self.prune_assist_imports(uri, &named, &name)?;
let imported = self.restore_imports(uri, &pruned, &name, &moved, reexport)?;
let (preserved, mut report) = restore_visibility(&imported, &name, &moved)?;

// The widenings the pass above cannot see, because the survey feeding it stops above an
// `impl`. Read off the text the assist actually produced, so a member it left alone is not
// reported as though it had moved.
```

### Findings

- **The anchor asked for lines 10–152; the assist relocated 10–116.** The remainder —
  `parse_planning_response_impl` and its callers — stayed in the parent and was **rewritten in
  place** to reach the new module through qualified `planning::` paths.
- **`survey_moved_items` ran before that happened**, over the original text and the full requested
  range. Every reference to `StructuredPlan` and `prd_value_looks_like_md_file_path` was inside the
  range at that moment, so `reached_from_outside` came back `false` for both.
- **`restore_visibility` narrowed both back to private** on that evidence. The assist had, in the
  meantime, created the outside references the survey concluded did not exist. `planning::StructuredPlan`
  and `planning::prd_value_looks_like_md_file_path` are then `E0603` — the exact two symbols the
  handoff named, from one cause rather than the three it listed.
- The crate already holds the corrective principle one line below, for `impl` members: *"Read off the
  text the assist actually produced."* It feeds the report and not the decision.
- This also disposes of the handoff's framing of F4 as two separate problems ("RA extract boundaries"
  and "private visibility"). They are one: a **partial relocation** the pipeline does not notice.

### Consequence for the plan

Milestone 3 changes shape. It is not a preflight refusing leftover references — a preflight could not
see this, because the leftover references do not exist until the assist has run. It is
`restore_visibility` deciding against the produced text, plus a guard that says so when an assist
relocates materially less than the anchor asked for.
