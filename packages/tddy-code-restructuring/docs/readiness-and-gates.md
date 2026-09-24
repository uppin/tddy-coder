# Readiness, and the gates that turn a silent success into a failure

An operation can go wrong without any single step failing: a server answers before it has loaded
what it needs, an assist copies code that cannot work where it lands, a run of accepted edits leaves
a tree the compiler rejects. Each of the mechanisms below exists to make that kind of result an
explicit failure, with its class and the reason, instead of a run that reports success.

| Mechanism | Where | Refuses as |
|---|---|---|
| Readiness waits on quiescence | `backends/rust/readiness.rs`, `backends/rust/chatter.rs` | (waits; `ServerNotSettled` / `CallerStopped` if it ends early) |
| Health gate | `readiness.rs` (`refuse_degraded_index`), `ServerChatter::degraded` | `ServerDefect` |
| Early-return refusal | `backends/rust/early_return.rs` (`refuse_early_returns`) | `SeamRefused` |
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

## The inferred-placeholder post-condition

An extract-method whose signature holds `_` (`fn resumed_session(req: _)`) is never applied.
`refuse_inferred_placeholder` checks the signature rust-analyzer wrote, on one line as it writes it,
and refuses as `ServerDefect`. With the readiness wait and the health gate ahead of it, this is a
backstop rather than the expected path.

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
- **`check --deep` does not run the compiler.** A clean deep check still does not mean the applied
  tree compiles; only `apply`'s gate says that.
- **Each writing apply pays two checks.** On a warm target directory they are incremental; on a cold
  one they are the price of a check, and they contend for the checkout's `target/` lock with the
  developer's own builds. The baseline runs before the per-operation static checks, so a plan that
  one of those would refuse still pays a `cargo check` first.
- **The degraded-index refusal names this repository's `./run-index-daemon` script**, which is host
  knowledge inside a library. It is kept because the advice is actionable where it is read.
