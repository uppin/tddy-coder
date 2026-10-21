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
| Silence bounds | `readiness.rs` (`silence_verdict`, `refuse_silence`, `keep_waiting_unless_stalled`; `SilenceBounds`, `READY_HOVER_BOUND`, `LOADING_SILENCE_BOUND`) | `ServerDefect` for a ready index that will not type a position; `ServerStalled` for a loading server gone silent |
| Untracked-move preflight | `apply.rs` (`untracked_move_sources`), `runner.rs` (`preflight_moves`), `runner/rehearsal.rs` (`resolve_and_preflight`, `untracked_move_anchors`) | `UntrackedMoveSource`, before the operation writes or journals anything |
| Early-return refusal | `backends/rust/early_return.rs` (`refuse_early_returns`), through `backends/rust/extracted_fn.rs` (`refuse_unliftable_returns`) | `SeamRefused` |
| Partial-cluster finding | `crate_move/cluster.rs` (`stranded_siblings`) | a static `check` finding: the cycle `apply` refuses (`refusals::refuse_a_dependency_cycle`) |
| Body-path finding | `crate_move/preconditions.rs` (`stays_behind_through_a_body`) | a static `check` finding: a body path to a module staying behind, which `apply` refuses as a dependency cycle |
| Merge finding | `crate_move/preconditions.rs` (`destination_already_has_the_module`) | a static `check` finding: the destination already declares or holds the module |
| Uncarriable-child findings | `crate_move/carried.rs` (`uncarriable`), run by `preconditions.rs` (`unrunnable`) | a static `check` finding per child a move cannot carry (see below) |
| Restricted child reached from outside | `crate_move/carried.rs` (`restricted_children_reached`), called by `cluster/gathering.rs` (`member_changes`) | `MalformedPlan`, at resolve time: `check --deep` and `apply` |
| Inferred-placeholder post-condition | `backends/rust.rs` (`refuse_inferred_placeholder`) | `ServerDefect` |
| Compile gate | `runner/compile_gate.rs` (`baseline_packages` for the baseline's set) | `BaselineDoesNotCompile`, `AppliedTreeDoesNotCompile` |
| Group gate | `runner/group_gate.rs` | `GroupDoesNotCompile` (the group is rolled back) |

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

**A wait that lasts says so.** Every polling wait owns a `Waiting` (`backends/rust/wait.rs`) and, on
a fixed heartbeat (`WAIT_HEARTBEAT`, 30 s), emits one line through the run's `progress` sink:
`still waiting (2m00s) — warming the crate index: rust-analyzer behind a shared client is still
loading; its last words, unchanged for 1m58s: "…"; furthest: …`. The line names the **stage**, the
**elapsed** time, **which server** (a pid this backend started, or "behind a shared client"), the
server's **last words** and how long they have been **unchanged** (`ServerChatter::quiet_for` — the
figure that separates a slow server from a stuck one), and the furthest phase reported
(`ServerChatter::furthest`, the highest percentage of any phase). A consumer that names what the server
is doing **now** reads `ServerChatter::current` instead: the phase reported on most recently, with the
highest percentage that phase reached — which the index daemon's `Warm` and `WatchRoot` streams do.
A consumer that lists **everything** the server is doing reads `ServerChatter::active_phases`: one
`ActivePhase { title, message, percentage }` per work-done token between its `begin` and its `end`, in
begin order — the title from its `begin`, the latest `message`, the token's highest percentage (`None`
until it reports one). A second `begin` on a running token restarts its phase at the end of the order; a
report whose `begin` was never seen is an untitled phase; the fold sees every report, including those the
console throttle does not print. After a notification gap, `ServerChatter::lost_sight` marks every token
in flight unconfirmed; a token heard from again is confirmed, and the next `quiescent: true` drops the
unconfirmed ones, so a phase whose `end` was lost does not stay listed for the server's life. The index
daemon folds one unthrottled chatter per root for the server's whole life to publish these. The
`"begin"` kind is read through one `BEGIN` constant. The cadence
is a collaborator a host or a test injects (`with_wait_heartbeat`, `Options.wait_heartbeat`,
`CodeIndexPorts.wait_heartbeat`), never a budget: nothing ends at it. A request already **in flight**
on the shared client is narrated too — `LspClientBridge` polls it in heartbeat-sized slices and beats
between them — so the one wait the poll loops cannot reach is not the silent one. A wait ended by
cancellation names its stage in `IndexingIncomplete.stage`, and the heartbeat reads the chatter and
the clock and writes neither readiness flag. A beat has three wordings for the server's state:
`is still loading`, `is ready and gives no answer here` (it said it was quiescent, read through
`ServerChatter::quiescent`, and the position is still untyped), and `has not said it is ready`, kept
for a server that never reported a status.

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

### Silence is bounded, total time is not

Every wait for type inference goes through `wait_until_answerable` (`wait_until_resolved` is its
refusing form for an operation acting at the position): renames, symbol anchors, the post-assist
rename, `extract_module` with `to_file`, the reference survey (`references_at`), the signature
rewrites and the extraction probes. The crate-index warm-up is `ensure_indexed`. Both poll through
`keep_waiting_unless_stalled`, and every silence they meet is judged by one pure decision,
`silence_verdict(loading, ready_silent_for, words_unchanged_for, &bounds)`, refused in one place,
`refuse_silence`. The bounds are `SilenceBounds { ready, loading }`, defaulting to
`READY_HOVER_BOUND` (30 s) and `LOADING_SILENCE_BOUND` (10 min), and injected like the heartbeat
(`RustBackend::with_silence_bounds`); they are not configurable from the daemon's YAML or the CLI.

- **Ready and untypable** (`Silence::ReadyUntypable`). A hover that stays `null` after the index is
  loaded and healthy, and that neither diagnostic above explains (`excused_silence`), will never
  change. Once the hover has been silent without a break for `bounds.ready` (`ready_silence` keeps
  that clock; any look that is not ready-and-silent stops it), the operation ends as
  `rust-analyzer's answer was unusable: the index is ready and rust-analyzer still gives no hover at
  <file>:<line> after …, so it cannot type this position`. Time before the index is ready does not
  count. The extraction probe asks at a position that can carry a hover (see
  [assist-output-repairs.md](assist-output-repairs.md#where-an-extraction-asks-and-what-extract_variable-binds)):
  a range opening on a `{` or a comment is probed at the first code inside it, so for extractions the
  bound is the backstop and not the expected path.
- **Loading and silent** (`Silence::Stalled`). While `ServerChatter::loading()` holds, the decision
  reads how long the server's last words have stood unchanged (`ServerChatter::quiet_for`, the
  heartbeat's `unchanged for`). Past `bounds.loading` the wait is refused as `ServerStalled { stage,
  server, last, quiet_seconds, furthest }` — `stage` is `warming the crate index` or `type inference
  at <file>:<line>`, `last` is the server's last words (`nothing reported` when it said none, the
  `NOTHING_REPORTED` constant the heartbeat shares). Any new word restarts the clock, so a load that
  keeps narrating is never refused here. It is judged in every 100 ms slice a poll sleeps in, not
  once per poll: a busy report can be folded away by the next poll's request before a once-per-poll
  look sees it.

Neither bound limits total time; cancellation is still the only bound on how long a wait lasts, and
`wait_heartbeat_acceptance::a_wait_has_no_deadline_however_many_beats_pass` pins that. The waits
outside these two (`settled_outline`, `locate_symbol`, `offered_assist`) end by cancellation or by
the server's answer only. `tddy-index-daemon`'s `status_of` maps `ServerStalled` to
`DeadlineExceeded`.

**Why there is a loading bound at all.** `#carve` 17 stage B2 had a one-op `rename_symbol` sit at
`waiting for type inference at the anchor` for over ten minutes at 0% CPU after `check --deep` of
the same plan answered, and every later request to the daemon queued behind it. Ten minutes is past
any single build script this workspace runs. A reproduction on the real workspace (three attempts,
2026-10-17) did not reproduce that hang: the type-inference wait answered at once each time, so the
two refusals are proved against `fake_lsp` (`silent_server_acceptance`), not a live server. It did
show rust-analyzer's own build-script `cargo check` holding `target/debug/.cargo-lock` while the
compile gate waited for it (see Known limitations).

## Documents are closed when an operation ends

An open document is the server's authority on its file: rust-analyzer stops reading that file from
disk until the document is closed. The server is often shared. `tddy-index-daemon` keeps one per
root for as long as it runs and serves a fresh backend per request, so a document left open would
outlive the run that opened it and go on answering for its file with the last text that run sent: a
trial import, or a rehearsed text whose new module files exist only in that run's overlay. Every
later request on that server would then read names as unresolved that resolve on disk, and refuse
correct seams.

So `backends/rust/documents.rs` owns `did_open`, records each document a backend opens, and closes
all of them when an entry point ends, whatever it ended with. `resolve` and
`outside_references` each run their body through `closing_what_it_opens`. When the entry point and
the close both fail, the entry point's error is the one returned. The live test is
`import_pass_acceptance::imports_the_parent_s_type_on_a_server_an_earlier_check_rehearsed_its_parent_on`:
a real `check --deep` of two seams on a settled server, then a seam naming the parent's type through
`super`, resolved on a fresh backend on the same server.

**Within one run.** Within one run, the files earlier operations wrote are the
exception: the backend opens them, at the text the run holds for them, before an operation's first
document (see [A seam after a seam](assist-output-repairs.md#a-seam-after-a-seam)), so an operation
after the first is resolved against the tree the plan has produced. The projected documents are closed
with every other when the operation ends, so the server forgets that text and it does not reach the next
run.

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

**Except a range that runs to the end of a named `fn` that returns a value, ending with its tail
expression**
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

**A function that returns `()` has no tail value to return**, so the exception does not apply to it
(`ends_in_a_unit_tail`): rust-analyzer rewrites the early `return` into a `ControlFlow` it matches at
the call, which names a type the tree does not import and leaves the caller a `return` that does
nothing. A range holding a `return` and running to the end of a `()` function is refused like any
other, in `check`, `check --deep` and `apply`, and the remedy is to start the range after the early
exit. The rewrite is not repaired. Whether the function returns `()` is read from its header text, a
`->` outside the parameters and generics and before any `where`.

**What `check`, `check --deep` and `apply` accept is decided by one classification**
(`extracted_fn::refuse_unliftable_returns`, over `guards::classify`), so the three agree and no server
is needed to refuse. Beyond the exception above it lets two more shapes through, and refuses every
other range holding a `return`:

- a range that is the whole body of a match arm or `if`/`else` branch whose construct is itself in
  tail position, recursively up to the function body, keeps its `return`s verbatim (they still return
  from the caller);
- a run of `return Err(..)` guards (or `return None` guards in an `Option` function) in the middle of a
  function is lifted into a function returning `Result<()>` (`Option<()>`) and called with `?`.

A value `return` in the middle of a function, error guards mixed with a value return, guards in a
function that returns no `Result`, guards beside a labelled `break` or `continue`, and a guard run
that declares a binding the code after it reads are each refused naming the line or binding. How the
lift is written is in
[assist-output-repairs.md](assist-output-repairs.md#a-run-of-guards-and-tail-position).

## The partial-cluster finding

`apply` refuses to move a module whose header still names a module staying behind in the origin:
the destination would depend on the crate it left, while the crate it left names the destination
back — through the facade, or, with `reexport: none`, from each caller `apply` re-points. A plain
`check` predicts that refusal statically, with no index (`stranded_siblings`), from the same pieces
`apply` uses: `header::repointed_header_at` and `refusals::origin_named_dependencies`, with the plan's
co-moving set and re-export resolution, so a path the origin only re-exports from another crate is
not an edge. The paths it reads are the top-level `use` header of each file the module carries, taken from the [path survey](path-survey.md); bodies and nested `use` items are `apply`'s alone. With a facade the finding fires whenever such a header path exists; with
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
## The body-path and merge findings

Both run in `move_preconditions`, which the static `check` tier and the partial-cluster analysis
share, so `check` and `check --deep` report them without an index.

**Body path.** `stays_behind_through_a_body` reads each carried file (the module's own and its directory
children) from the [path survey](path-survey.md) and takes the first path that is in a body, outside `#[cfg(test)]`, defined in the origin crate, and
inside a module that stays behind. A path to an item at the crate root is inside no module and is not
named; a path the origin only re-exports from another crate is defined elsewhere; a path under
`#[cfg(test)]` is no edge. The finding names the file, the path as written, its line, the module and
both crates, and its remedy is to cut the body's dependency — never a cluster.

*What travels, and what has already left.* A module the operation itself moves — every anchor of a
cluster, through `member_op`, which keeps the whole set in `also` — travels with it. A module an
**earlier operation of the plan** moves to the same destination has left the origin too
(`moved_by_earlier_operations`): a plan the header pass accepts is not refused here. That is the same
question `gone_by_then` answers for the header, asked of the plan's operations directly, because
`gone_by_then` reads a list built by running these preconditions and cannot be asked from inside them.

**Merge.** `destination_already_has_the_module` reports a destination whose root already declares
`mod <name>;`, or which already has a file at the target path. Moving into it would be a merge, which
no operation performs, so the finding says that rather than suggesting a cluster.

## The uncarriable-child findings and the restricted child

A moved module carries its directory children ([path survey](path-survey.md#carried-files)).
`carried::uncarriable` reports, statically and one finding per child, naming its file, each way a child
cannot be carried: a `mod x;` that leads to neither `x.rs` nor `x/mod.rs`; a carried file that places a
`mod` with `#[path]`; a target in the destination that already exists, which would be a merge; a child
that another operation of the plan moves on its own, whether to another destination or after its parent
has left; and a child whose body reaches a module that stays behind. It runs for an operation only once
`move_preconditions` passes, because a refused operation is already reported once. A member that another
member of the same operation carries is not read on its own.

**A restricted child reached from outside** needs the reference set, so it is refused at resolve time, by
`check --deep` and `apply`, and not by plain `check`. A carried child declared `pub(crate)`, `pub(super)`,
`pub(in …)` or private, that a file outside the moved tree names through the child's own path
(`crate::a::b::Item`), would be private to the destination after the move (`E0603`). The refusal names the
child and each referring file, and nothing is written. A child's restriction is read where the child is
declared, including inside an inline module of a carried file; an enclosing inline module's own
visibility is not read.

**The cycle refusal and the carried tree.** The edges of every carried file reach
`refusals::refuse_a_dependency_cycle`, which lists the paths that forced the refusal and names the
module's own file as the one that "still names" the origin, whichever carried file holds the path.

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

- **Baseline, before anything is written.** `refuse_a_broken_baseline` checks the packages
  `baseline_packages` returns: those owning every file the plan names (snapshot and anchors), and the
  destination package of every cross-crate move (`move_module_to_crate`, `move_cluster_to_crate`,
  `move_test_binary_to_crate`), read as the `[package] name` of `<to>/Cargo.toml` through
  `Destination::read`. A `to` with no manifest adds nothing (a move that creates its crate is judged
  by the operation); a manifest that is there but cannot be read, or declares no `[package] name`, is
  refused. Other kinds' `to` is not read. So a destination that was already broken is reported as
  `cargo check --all-targets -p destination -p origin` failing before the plan runs, not blamed on the
  plan by the result check. A failure is `BaselineDoesNotCompile`, and nothing
  is written, so a pre-broken tree is never blamed on the plan. It runs as the `before_writing` gate of
  `open_run_after`: after the cheap read-only refusals (git worktree, repo-scoped journal,
  `JournalExists`, snapshot), after item anchors are resolved (`open_run_resolving_anchors`, the one
  order both apply paths use — see [item-anchors.md](item-anchors.md)), and before `.restructure/` is
  created. It is skipped for a dry run and for
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

**Nothing is rolled back** for an ungrouped operation (a [transactional group](#transactional-groups) is rolled back). The edits stay on disk and in the journal for inspection, and no rollback
command exists. The message says how: restore the touched paths from git (`git checkout HEAD --` what
HEAD holds, delete what the run created, `git reset` what it staged) and remove the journal so the
plan can run again.

**Cancellable.** The check is spawned, its stderr drained on a thread, and `try_wait` polled against
the run's `CancellationToken`. On cancel the child is killed and the run returns `CallerStopped`
(the daemon's `Status::cancelled`); a cancel during the result check first tells the progress sink
that the applied edits are on disk and unchecked. Cargo's own `rustc` children are not killed; they
finish the unit they are building into `target/`.

**The tidy did not run.** A failed result check skips the tidy, so the message says so: the first line
of `AppliedTreeDoesNotCompile`'s `errors` is "the tidy did not run, so the written files were neither
tidied nor formatted; the failing check reported N `unused import` warning(s) in them" (`untidied_note`;
N counts the failing check's `short` warnings in the files the run wrote, and is `0` when it reported
none). A successful apply says nothing about a skipped tidy. The tidy never runs over a broken tree.

**What it reports.** Lines starting `error` and lines holding `: error` (the short format's
`path:l:c: error[E…]`) are kept. When no line matches, the whole stderr is returned, so a failure is
reported in full rather than masked.

**Recorded.** Every `cargo check` this runs — the baseline, the result check and the tidy's own —
goes through the run's `SpawnRecorder` under the purpose `compile-gate`, so the record holds what
was executed and how each ended. The baseline, the result check and the tidy's are run-level and carry no
operation; a group's end-of-group check is started through `spawns.for_group(name)` and carries
`group`. A check killed on cancel is recorded with the signal that killed
it; a tree that fails the baseline is recorded with the exit status the compiler returned (101 for a
type error). See [What a run executes](../README.md#what-a-run-executes).

**Classes.** Neither failure is malformed input, a refused seam or an unusable server answer, so each
is its own `RestructureError` variant. `tddy-index-daemon`'s `status_of` maps the baseline to
`FailedPrecondition` (the same request fails until the tree is repaired) and the applied tree to
`Internal` (the executor produced it).

## The untracked-move preflight

A move goes through `git mv` so history survives, and `git mv` refuses a file git does not track.
Inside one edit the order is creates, then text changes, then renames, so a move of an untracked file
used to fail after its text edits were on disk, with the operation already journalled in flight —
and the next run refused the journal as indeterminate. The preflight refuses it first.

- **When.** `runner::preflight_moves(index, id, resolved, root, overlay, spawns)` runs on each
  resolved edit **before** `GroupRun::enter` and before the journal's `in_flight` record, in both
  apply loops (`store_run.rs` for the CLI, `tddy-index-daemon/src/apply.rs` for the daemon), right
  after resolution and before the dry-run branch. A refusal leaves the operation's files and the
  journal as the previous operation left them, so after `git add` the same plan applies with no
  `--resume` and no clean-up. Operations of the run that already committed stay committed; a group is
  rolled back whole, as for any member failure.
- **What is tracked.** `apply::untracked_move_sources` runs one `git ls-files --error-unmatch -z --
  <paths…>` per edit that renames, then one per path when that fails, to name each untracked one
  (`LS_FILES_UNMATCHED`, exit status 1, means "not tracked"; any other failure is `MalformedPlan`, as a
  failed `git mv` is). A path is tracked when git knows it (an intent-to-add or a staged rename
  counts), when it is a `Create` path of the same edit, or when the run's `Overlay` introduced it
  (`Overlay::introduced`: dry run and rehearsal, where nothing reached disk). Its `git` is recorded
  for its operation (`spawns.for_operation(index, id)`).
- **Parity.** `apply --dry-run` (the same call, overlay populated) and `check --deep`
  (`Rehearsal::resolve_and_preflight`, which turns the refusal into a finding) refuse the same
  operation the same way. Static `check` reports an untracked anchor file of the three cross-crate
  move kinds (`untracked_move_anchors`, chained into `check_plan`'s static move findings through
  `static_move_findings`), including a cluster's `also`. A file not on disk is not judged statically,
  since an earlier operation may create it; `reparent_module` anchors the old parent's `mod` line, not
  the moved file, so its untracked source is reported by `check --deep` and `apply` only.
- **The refusal.** `UntrackedMoveSource { op, paths }` reads: operation *N* moves *paths*, which git
  does not track — a move goes through `git mv` so history survives. `git add` it and apply again.
  Nothing of operation *N* was written. `status_of` maps it to `FailedPrecondition`.
- **Not done.** No automatic `git add` of the author's file and no fall back to `fs::rename`: either
  would be a fallback the author did not ask for.


## Transactional groups

Some refactors only compile once several operations have landed. A plan gives such operations the
same `"group"` (`RefactorOp.group`); the members must be **consecutive**, or `Plan::parse` refuses the
plan as malformed. `RefactorOp` refuses any field it does not define (`deny_unknown_fields`), so a
misspelt `group` is an error rather than a plan that runs ungrouped. Ungrouped operations keep the
end-of-run compile gate above and its leave-on-disk contract.

**Journal.** A group writes, in this order: `group_started` (the group's id and the plan index of every
member), then for each member `pre_imaged` — the text, before the group first touched it, of every file
the member's edit touches that no earlier member has (`PreImage { path, contents: Option<String> }`;
`None` for a file the group creates, so restoring it removes the file; a rename is two pre-images) —
written **before** that member's `in_flight`, and finally `group_completed` or `group_rolled_back`.
Records are serde-default, so a journal without groups loads and an ungrouped record serialises as
before. `Journal::open_group` returns the group started and never closed, with its pre-images.

**The gate.** `GroupRun` (`runner/group_gate.rs`) is what both apply loops carry from a group's first
member to its last: `enter` begins the group and captures pre-images, `settle` records the member and,
at the last one, runs `gate_group`: `cargo check --all-targets` over the packages owning every file the
members journalled an edit to, under the run's `CancellationToken`. On a pass it journals
`group_completed` and hands every member's resolution back, in plan order, for the plan store to refresh
together; members are not refreshed while the group is open. A group-less operation is `Settled::Ready`
at once.

**The rollback.** `roll_back_group` restores the pre-images in reverse order (contents rewritten, created
files removed, renamed files moved back), puts the ledger checkpoint back to the last operation before
the group, and only then journals `group_rolled_back`: a checkpoint behind the journal is the lag a resume
accepts, one ahead of it is refused, so a crash between the two stays resumable. An empty directory a
created file lived in is left behind.

**What rolls back.** Any failure while a group is open: a failed gate (`GroupDoesNotCompile { group,
errors }`, the compiler's error lines), a member that cannot be resolved or committed, a check that cannot
run. The original error is returned; only the gate's names the group. `CallerStopped` is the exception: the
group stays open in the journal, and the daemon, which sees a cancelled token between members, rolls it
back immediately.

**Resume.** `roll_back_an_open_group` runs before a continued run (`--resume`, `--from`) reads a single
anchor, so the gate judges members one run applied: the partial group is undone and applied again whole.

**Reporting.** A group's members are reported as applied only after the group is kept: the command line
as the members' visibility lines and "applied" lines each followed by `console::group` (`   group: <name>`),
the daemon as `OperationApplied` events with `group` set. A rolled-back group reports none of its members.

**Elsewhere.** `check --deep` merges the findings of a group's members into one naming the group
(`one_finding_per_refused_group`, `check_entry_points.rs`). `stop_after` is judged where a group would
begin; all members count toward the limit.

## The tidy

After the result check passes, `apply` tidies the files it wrote (`runner/tidy.rs`, with
`runner/tidy/{diagnostics,gating,format}.rs`). An operation orphans `use` lines, and the lint gate
(`cargo clippy -- -D warnings`) fails on every one, so the tidy runs where the compile gate has just
proved the tree builds.

**The compiler decides.** The engine never prunes an import on its own reading of a file, because a
trait import can be needed invisibly (`use std::fmt::Write;` has no name in the source that uses it).
rustc's `unused_imports` lint is trait-aware and carries machine-applicable removal suggestions, so
those are the evidence. Only files the run wrote are edited, only a suggestion rustc marks
`MachineApplicable` is applied, and the loop is bounded by `MAX_ROUNDS` (removing one import can orphan
another). Whatever the compiler still warns about is reported, never fixed.

**An import only tests use.** `unused_imports` fires per compilation unit, so an import that only a
`#[cfg(test)] mod tests { use super::*; … }` reads is reported by the library unit, and removing it breaks
the test unit. Such an import is gated `#[cfg(test)]` instead; a group member becomes its own gated item
(`gating.rs`). A glob binds no name an error could quote, and an error over a trait method
(`write_str`) names the method, not the trait. A repair therefore gates every import a build reads while
another reports it unused (`to_gate_in_a_repair`: the spans the errors name, plus `read_by_a_unit`),
whether or not an error quotes it. A glob every build reports unused is still removed.

**Statements are rewritten from sets, not from spans.** A library is checked as itself and with its
tests, and each unit words its removal of one group its own way (`a, ` here, `, b` there), so the member
edits of the two units overlap. The tidy counts the units (one `compiler-artifact` per unit for a
`target.src_path`) and rebuilds a statement the units disagree on from the *names*:

| Reported unused by | Result |
|---|---|
| every unit | removed |
| fewer units than build the file | read by the rest: gated in a repair, kept otherwise |
| none | kept |

Nothing is applied as two overlapping edits, and `apply_fixes` fails loudly on an edit it cannot apply
instead of dropping it.

**A repair iterates while it improves.** When a round's edits break the re-check, the round's files are
restored from the bytes held in memory and redone with every import gated that the errors quote or a
unit reads. The redo is repeated while each redo leaves fewer quoted names
failing, at most `MAX_REPAIRS` times; a redo that stops improving is undone and **fails the run**,
saying so. A nested group is left in place and reported. The tidy never leaves a broken tree behind and
never touches the plan's own edits.

**An unused `mut`.** rust-analyzer reports no unused `mut`, not even with its experimental diagnostics,
so the one an `extract_method` copies into the new function, or leaves in its caller, is the
compiler's. After the imports are tidied and before `rustfmt`, the diagnostics of the last check are
filtered to rustc's `unused_mut` with a `MachineApplicable` fix in a file the run wrote
(`runner/tidy/unused_mut.rs`), and the fixes are applied with the files' bytes held in memory. The tree
is re-checked: a clean re-check reports ``tidied: removed N unused `mut`(s) from <file>`` per file, and a
re-check that fails restores the bytes and fails the run, as the import step does. One round is enough:
removing a `mut` cannot make another unused. A dry run writes nothing, so removes nothing.

**Formatting.** `rustfmt` runs over every file the run wrote, with its package's edition, so the
`cargo fmt --all --check` step of CI's lint job finds nothing the engine left. Each `rustfmt` and
each `cargo check` the tidy runs is reported through the run's `SpawnRecorder` (`tidy-format` and
`compile-gate`), so the record holds them beside the apply's own `git` — see
[What a run executes](../README.md#what-a-run-executes).

**The shape that defeated the first version.** A split into nine modules left eleven wide `named`
facade groups; the library unit and the test unit reported different name sets for each. The two units'
member spans overlapped, one of them was skipped silently, a name stayed in a group that the failed
re-check never quoted, and the redo then removed it, which broke the tests that read it. A redone round
was never repaired a second time, so the run failed after a correct split.

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
  developer's own builds — and with rust-analyzer's own build-script `cargo check`, which shares
  `target/` with the gate: measured at roughly 47 s and 60 s of lock wait on this workspace
  (2026-10-17), not permanent. rust-analyzer has no separate `cargo.targetDir`. The baseline runs before the per-operation static checks, so a plan that
  one of those would refuse still pays a `cargo check` first.
- **The degraded-index refusal names this repository's `./run-index-daemon` script**, which is host
  knowledge inside a library. It is kept because the advice is actionable where it is read.
