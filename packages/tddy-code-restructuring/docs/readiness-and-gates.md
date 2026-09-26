# Readiness, and the gates that turn a silent success into a failure

An operation can go wrong without any single step failing: a server answers before it has loaded
what it needs, an assist copies code that cannot work where it lands, a run of accepted edits leaves
a tree the compiler rejects. Each of the mechanisms below exists to make that kind of result an
explicit failure, with its class and the reason, instead of a run that reports success.

| Mechanism | Where | Refuses as |
|---|---|---|
| Readiness waits on quiescence | `backends/rust/readiness.rs`, `backends/rust/chatter.rs` | (waits; `ServerNotSettled` / `CallerStopped` if it ends early) |
| A wait ended by the server's own diagnostics | `readiness.rs` (`wait_until_answerable`) | `SeamRefused` for an operation at inactive code or in a file no module tree reaches |
| Documents closed after each entry point | `backends/rust/documents.rs` (`closing_what_it_opens`) | (refuses nothing; stops a shared server answering from a finished run's text) |
| Health gate | `readiness.rs` (`refuse_degraded_index`), `ServerChatter::degraded` | `ServerDefect` |
| Early-return refusal | `backends/rust/early_return.rs` (`refuse_early_returns`) | `SeamRefused` |
| Partial-cluster finding | `crate_move/cluster.rs` (`stranded_siblings`) | a static `check` finding: the cycle `apply` refuses (`refusals::refuse_a_dependency_cycle`) |
| Inferred-placeholder post-condition | `backends/rust.rs` (`refuse_inferred_placeholder`) | `ServerDefect` |
| Compile gate | `runner/compile_gate.rs` | `BaselineDoesNotCompile`, `AppliedTreeDoesNotCompile` |

There is **no opt-out flag** for any of them.

## Readiness

`ensure_indexed` and `wait_until_resolved` decide when the server may be asked. A non-null hover is
not enough: rust-analyzer answers hover while it is still running build scripts, so for a few
seconds `OUT_DIR` types do not exist for it, no name shows as unresolved, and an extract-method
writes `req: _`.

`ServerChatter` folds every `experimental/serverStatus` the server sends. `loading()` is true while
the last status said `quiescent: false`, and both waits keep polling while it holds. A server that
never sends the extension still gets through on hover alone. `client_capabilities()` advertises
`serverStatusNotification`, and `server_settings()` leaves `cargo.buildScripts.enable` and
`procMacro.enable` at their defaults, which are on.

A status is sent only on a **transition**, and `drain_notifications` is destructive and capped, so a
second backend on a warm, already-drained client would never see one. `tddy-lsp` therefore keeps the
latest status on the client (`LspClient::server_status`), and `LspClientBridge::notifications_to_fold`
appends it after whatever it drained. The same readiness and health rules hold on the cold path and
against a warm index daemon, because the daemon builds its backends through `runner::registry_for`.

**A `null` hover that will never change is read from the server's diagnostics, not waited out.**
Two kinds of position never resolve, however long the index has been ready, and rust-analyzer says
which in its pull diagnostics (`textDocument/diagnostic`). Once the index is loaded and a hover is
still `null`, `wait_until_answerable` fetches that report, once per poll:

- **`inactive-code`**: an item under a `#[cfg]` the server has switched off (`#[cfg(not(unix))]` on
  macOS). The syntax outline lists it, so a caller survey (`move_module_to_crate`,
  `move_cluster_to_crate`, the `extract_module` reach) asks about it. The wait ends with
  `Answerable::Inactive`: the survey still asks the server for the item's references, takes the
  empty answer, and says on the progress line that only code under the evaluated cfg is surveyed. An
  operation acting *at* such code, such as a rename, is refused as `this seam cannot be cut here:`,
  naming the file and line and quoting the server.
- **`unlinked-file`**: a file no crate's module tree reaches, as the server has loaded it. The
  operation is refused as `this seam cannot be cut here:`, naming the file and quoting the server.

Before the index is loaded, a missing diagnostic proves nothing, so the wait goes on. No timeout is
involved: the server's own answer ends the wait.

## Documents are closed when an operation ends

An open document is the server's authority on its file: rust-analyzer stops reading that file from
disk until the document is closed. The server is often shared. `tddy-index-daemon` keeps one per
root for as long as it runs and serves a fresh backend per request, so a document left open would
outlive the run that opened it and go on answering for its file with the last text that run sent: a
trial import, or a rehearsed text whose new module files exist only in that run's overlay. Every
later request on that server would then read names as unresolved that resolve on disk, and refuse
correct seams.

So `backends/rust/documents.rs` owns `did_open`, records each document a backend opens, and closes
all of them when an entry point ends, whatever it ended with. `resolve`, `anchor_for` and
`outside_references` each run their body through `closing_what_it_opens`. When the entry point and
the close both fail, the entry point's error is the one returned. The live test is
`import_pass_acceptance::imports_the_parent_s_type_on_a_server_an_earlier_check_rehearsed_its_parent_on`:
a real `check --deep` of two seams on a settled server, then a seam naming the parent's type through
`super`, resolved on a fresh backend on the same server.

**What this does not change.** Within one `check --deep`, an operation after the first is still
resolved against overlay text whose new files the server cannot see. The server forgets that text
when the operation ends, so it does not reach the next run.

A file written **between** requests — a module an earlier `apply` created, or a hand edit — reaches a
warm server because `tddy-index-daemon` tells it: before handing a warm server to a request, the
daemon sends `workspace/didChangeWatchedFiles` for every `*.rs`, `Cargo.toml` and `Cargo.lock` file
created, changed or deleted since the previous request (see
[code-index-service.md](../../tddy-index-daemon/docs/code-index-service.md#warm-state-per-workspace-root)).
A close sent before an `apply` writes, or for a file the server never opened, would not do it, and
rust-analyzer's own watcher does not see these files on this workspace.

## The health gate

`ServerChatter` also records the latest `health` and `message`. At every point readiness declares
ready, `refuse_degraded_index` refuses with a `ServerDefect` when the health is anything but `ok`,
quoting rust-analyzer's message (whitespace collapsed) and advising a restart from the dev shell's
whole environment.

**`warning` fails too.** A failed build script arrives as `warning` ("Failed to run build scripts of
some packages"), and an index in that state answers without the code it could not build. The other
warnings rust-analyzer sends (a manifest change not yet reloaded, build scripts or proc macros
needing a rebuild, a configuration error, no workspace discovered) likewise mean the graph is not the
tree on disk.

The usual cause of a degraded index is the environment the server was started in: rust-analyzer runs
build scripts and proc macros in it, so they can fail to link there while `cargo check` in the dev
shell succeeds. That is why `run-index-daemon` gives the daemon the dev shell's whole environment.

## Early returns in `extract_method`

rust-analyzer's "extract into function" copies a `return` in the range verbatim into the new
function, whose return type is not the enclosing function's, so the result is `E0308`.
`refuse_early_returns` refuses such a range as `SeamRefused`, naming the lines, before the assist
runs. It runs in the static `check` tier and in `resolve`, so `check`, `check --deep` and `apply` all
refuse it without a server.

Detection is **lexical**:

- strings, raw strings, character literals and comments are masked with the lexer the test-binary
  move already has (`readable_spans`, shared through `masked_to_code`);
- closures (block, `-> T` block and expression bodies, with `|` read as a closure only where an
  expression may start), `async` blocks and nested `fn name` open a body whose `return` does not
  count;
- only the range is scanned, from depth zero, so statements lifted out of a closure body cannot carry
  that closure's `return`.

**Except a range that runs to the end of a named `fn`, ending with its tail expression**
(`runs_to_the_end_of_a_function`). There rust-analyzer keeps the `return` verbatim, gives the new
function the caller's return type (the tail's), and the call replaces the range as the caller's tail,
so a `return` means what it did; a `?` in that tail propagates the same error type. The range may not
end with `;`, only whitespace or a comment may follow it before a `}`, and that brace must close the
body of a named `fn` — not an `if` block, a `match` arm, a closure passed to a call or an `async`
block, which are refused as before. A range ending with the body's last `return …;` statement is
refused too: rust-analyzer then rewrites every `return` into an `Option` matched at the call, leaving
the caller with no tail (`E0317`). A return type rust-analyzer spells differently from the caller's
(an `impl Trait`) cannot be told from the text; the compile gate catches it. The refusal's remedy
offers all three ways out: cut the range so it holds no `return`, end it before the first one, or run
it to the end of the function's tail expression.

## The partial-cluster finding

`apply` refuses to move a module whose header still names a module staying behind in the origin:
the destination would depend on the crate it left, while the crate it left names the destination
back — through the facade, or, with `reexport: none`, from each caller `apply` re-points. A plain
`check` predicts that refusal statically, with no index (`stranded_siblings`), from the same pieces
`apply` uses: `header::repointed_header` and `refusals::origin_named_dependencies`, with the plan's
co-moving set and re-export resolution, so a path the origin only re-exports from another crate is
not an edge. With a facade the finding fires whenever such a header path exists; with
`reexport: none`, only when some file staying behind names the moved module, and it lists them.

**A module staying behind that names the moved one is not a finding.** A facade keeps its
`crate::…` path resolving, and without a facade `apply` re-points it, so it compiles either way.

**Staying behind is read at each operation's point in the plan** (`gone_by_then`), because `apply`
runs one operation at a time. A module moved by the same operation or an earlier one has left the
origin; one moved by a later operation is still there. So a mutually-referencing set written as one
`move_module_to_crate` per member is reported at its first operation, naming the later operation that
moves the sibling and giving `move_cluster_to_crate` (this module as anchor, the rest in `also`) as
the remedy; the same set as one `move_cluster_to_crate` reports nothing. A module `move_preconditions`
already refuses is left out of the set. The plan-level rule is in
[plan-schema.md](../../../.agents/skills/code-restructuring/references/plan-schema.md).
## The inferred-placeholder post-condition

An extract-method whose signature holds `_` (`fn resumed_session(req: _)`, `-> Vec<_>`,
`-> (_, _)`) is never applied: that is `E0121` in an item signature. `refuse_inferred_placeholder`
checks the signature rust-analyzer wrote, on one line as it writes it, and refuses as `ServerDefect`.
With the readiness wait and the health gate ahead of it, this is a backstop rather than the expected
path.

**The elided lifetime `'_` is not a placeholder.** A `_` straight after `'` is read as a lifetime, so
a borrowed view passed as a parameter (`state: RosterState<'_>`) is accepted, while a real `_` beside
it (`state: View<'_>, value: _`) is still refused. The check applies to signatures only: a `let` may
legally carry `_` (`collect::<Vec<_>>()`), which is why `extract_variable`'s binding is not checked.

## The compile gate

Every **writing** `apply`, on both apply paths (`runner::apply` for the CLI, and the index daemon's
`apply_plan`), is bracketed by `cargo check --all-targets --message-format short -p …`:

- **Baseline, before anything is written.** `refuse_a_broken_baseline` checks the packages owning
  every file the plan names (snapshot and anchors). A failure is `BaselineDoesNotCompile`, and nothing
  is written, so a pre-broken tree is never blamed on the plan. It runs as the `before_writing` gate of
  `open_run_after`: after the cheap read-only refusals (git worktree, repo-scoped journal,
  `JournalExists`, snapshot) and before `.restructure/` is created. It is skipped for a dry run and for
  a run continuing a journal (`--resume`, `--from`), whose tree already holds the earlier run's edits.
- **Result, after the last operation.** `refuse_a_broken_result` checks the packages owning every
  file the journal records a completed edit to, this run's and an earlier run's on resume. A failure
  is `AppliedTreeDoesNotCompile`, carrying the compiler's error lines, the touched paths and the
  plan's journal directory. The daemon judges before it emits its outcome event, so a stream never
  ends with "applied N of N" over a broken tree.

**`--all-targets`**, because moves re-point imports that test targets use and a moved test binary is
a test target; a lib-only check passes exactly the breakage these operations cause.

**Packages are found by walking up** to the nearest manifest with a `[package]` name
(`declared_package_name`), because a file a rename moved away no longer exists for `cargo metadata`
to place.

**Nothing is rolled back.** The edits stay on disk and in the journal for inspection, and no rollback
command exists. The message says how: restore the touched paths from git (`git checkout HEAD --` what
HEAD holds, delete what the run created, `git reset` what it staged) and remove the journal so the
plan can run again.

**Cancellable.** The check is spawned, its stderr drained on a thread, and `try_wait` polled against
the run's `CancellationToken`. On cancel the child is killed and the run returns `CallerStopped`
(the daemon's `Status::cancelled`); a cancel during the result check first tells the progress sink
that the applied edits are on disk and unchecked. Cargo's own `rustc` children are not killed; they
finish the unit they are building into `target/`.

**What it reports.** Lines starting `error` and lines holding `: error` (the short format's
`path:l:c: error[E…]`) are kept. When no line matches, the whole stderr is returned, so a failure is
reported in full rather than masked.

**Classes.** Neither failure is malformed input, a refused seam or an unusable server answer, so each
is its own `RestructureError` variant. `tddy-index-daemon`'s `status_of` maps the baseline to
`FailedPrecondition` (the same request fails until the tree is repaired) and the applied tree to
`Internal` (the executor produced it).

## Known limitations

- **A `return` a macro expands to is not seen** (`bail!`, `ensure!`). `apply`'s compile gate catches
  it; `check --deep` does not.
- **A leading `|` in a match arm** (`match x { | A => … }`) is misread as a closure, so a `return` in
  that arm would not be counted.
- **`break` and `continue` that target a loop outside the range** are the same hazard as an early
  return and are not refused.
- **Callers are surveyed only under the cfg rust-analyzer evaluated.** A caller inside code that is
  inactive on this host is not re-pointed by a crate move; the progress line says so when the moved
  module holds inactive code itself.
- **`check --deep` does not run the compiler.** A clean deep check still does not mean the applied
  tree compiles; only `apply`'s gate says that.
- **Each writing apply pays two checks.** On a warm target directory they are incremental; on a cold
  one they are the price of a check, and they contend for the checkout's `target/` lock with the
  developer's own builds. The baseline runs before the per-operation static checks, so a plan that
  one of those would refuse still pays a `cargo check` first.
- **The degraded-index refusal names this repository's `./run-index-daemon` script**, which is host
  knowledge inside a library. It is kept because the advice is actionable where it is read.
