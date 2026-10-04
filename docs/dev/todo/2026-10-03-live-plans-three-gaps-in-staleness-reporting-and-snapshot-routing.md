# 2026-10-03 — live plans: known gaps in staleness reporting and its lifetime

**Category:** Future enhancement (gaps the first implementation documents rather than closes)
**Source:** `#live-plan` 7/15, [#539](https://github.com/uppin/tddy-coder/pull/539) — the implementation of
the plan store's fold, re-resolution and stale-operation reporting. Each gap carries a
`TODO(live-plans)` marker in the code that names this file.

None of them makes a plan wrong or an `apply` unsafe on its own: `Apply` refuses a stale operation at
or after its start, and for item anchors the resolver's fingerprint refusal is the last line at apply
time. The gaps are on the *reporting* and *lifetime* side. Items 1 and 3 carry a
`TODO(live-plans)` marker in the code that names this file; items 4–8 came out of the validation
review of the node and have none, because each is a design choice the first cut made on purpose. The
numbering is the entry's own and is kept, so a reference to an item still finds it; there is no item 2.

## 1. An item anchor in a file that was deleted does not go stale

`tddy-index-daemon/src/plan_upkeep.rs` (`reresolve_loaded_plans`) re-resolves a loaded plan's item
anchors for the `.rs` files the tree-change path saw changed. A file the daemon sees **deleted** is
not passed on: the resolver has no answer for a file that is not there, and the store cannot tell
"the file is gone" from "the file changed". So a plan whose anchored item lived in a deleted file keeps
its anchor, reports nothing, and is refused only when `apply` itself tries to resolve it.

**What would close it.** Teach `PlanStore::reresolve_files` to take the set of files that no longer
exist as a second argument (or to probe `Path::is_file` itself) and mark every op anchored there
`StaleReason::ItemNotFound` — the reason already exists and already renders as
`item not found in <file>`. A test: load a plan anchored in `a.rs`, delete `a.rs`, let the tree-change
path observe it, and read `ListPlans` — the op is stale.

## 3. An operation the plan already ran can be reported stale

`plan_store/live/fold.rs` folds a foreign edit into **every** operation of another held plan, because
the store holds no journal and so cannot tell which operations have already run. An operation that
already ran can be marked stale by an edit overlapping where it used to anchor. `Apply` is not
affected — a run only refuses the operations at or after its own start — but `ListPlans` and
`PlanStatus` show the operation as stale, which reads as a problem where there is none.

**What would close it.** Give `PlanStore` the applied count per plan (the daemon's apply loop already
knows it when it records the op) and fold only the pending suffix; ops before it keep whatever the
journal says. A test: apply op 0 of plan B, apply an overlapping edit from plan A, and `ListPlans`
reports B's op 0 as not stale.

## 4. Stale state does not outlive a load

`Liveness` (`plan_store/live.rs`) is held in memory beside the plans and dropped when a plan is
unloaded. The flushed plan file stays a plan — a stale op's anchor is left at its pre-edit
coordinates — so after an unload and reload, or a daemon restart, an op that was `EditedBy` another
plan is no longer reported stale. For **item** anchors that is safe: the resolver's fingerprint
refusal catches the changed item at apply. A `range` or `symbol` anchor in a v2 plan is not
fingerprinted, so it would run at its old coordinates; that is how v2 plans have always behaved (drift
is reported, never refused), and the compile gate is the last line.

**What would close it.** Re-derive on load: for every op, compare the anchored file's current hash to
the v2 `files` hint and, for an anchor that is not an item anchor, mark it stale when the hash differs;
or persist the stale reasons in the journal. Until then the changeset says what is true: staleness is
derived by re-resolution and held for the life of a load.

## 5. A failed re-resolution fails an unrelated request and loses the change

`tddy-index-daemon/src/index.rs` (`client_for`) awaits `reresolve_loaded_plans(..)?` after the tree
snapshot has already advanced. A transient language-server error while re-resolving fails a `Check`,
`Apply` or `Anchors` request that had nothing to do with it, and the changed files are never observed
again, so those plans silently stay unrefreshed.

**What would close it.** Advance the recorded tree only after the re-resolution succeeds (the server
must be told first, so the order is: tell, re-resolve, then record), or keep the unobserved paths in
the state and retry them on the next request.

## 6. `ListPlans` and `PlanStatus` only see a hand edit after another request

Hand-edit staleness is picked up on the first request that reaches `client_for` (`Check`, `Apply`,
`Anchors`). `ListPlans` and `PlanStatus` never call it, so right after a hand edit they report the
pre-edit state until one of those runs; and a server that was reaped while idle returns an empty
change set from `record_use`, so edits made in that window are never re-resolved (they are still
refused at apply by the fingerprint).

**What would close it.** Run the tree-change pass from the two reporting RPCs as well, and treat a
fresh server as "everything may have changed" for the loaded plans' files.

## 7. The applied plan's own v2 `files` hints are not rewritten

`refresh_after_op` is the plan-store node's and was not changed here, so the plan an operation was
applied *from* keeps its own v2 `files` hints (other plans' hints are rewritten). A v2 plan therefore
reports hint drift on its own files right after its own apply. `followed_hints` (`live.rs`) already
does the work for the other plans.

**What would close it.** Call it for the applying plan from the same place, with the ledger of the op
just applied.

## 8. Two behaviours differ from what `restructure snapshot` promises

- **Bytes.** `snapshot_resolving` rebases through `rebase_plan_file`, which re-serialises the plan with
  `flush`; `snapshot`'s own documentation promises the bytes are preserved apart from the header.
- **The "already" line.** `snapshot()` runs after the rebase's flush, so its `rewritten` flag is
  probably `false` and the console says "already snapshots the working tree" even though hints were
  rewritten.

**What would close it.** Have `rebase_plan_file` report what it changed and let `snapshot` render
that; pin both with a test through `snapshot_resolving`.

## Cleanups the review noted, none worth a node on its own

`HeldPlanRow` is a 4-tuple (should be a struct); the `PlansResponse` → row mapping and the stale-pair
mapping are copied four times across `render.rs`, `index_console.rs` and `console.rs` (a
`From<&StaleOp>` removes all of them); `resolved_or_left_as_written` is now also how a stale verdict is
decided, so its name misleads; `fold_item` and `rejudge_anchor` share the "hint from the resolved
range" block and `fold_items` shares a per-item loop with the `Items` arm of `rejudge_anchor`.
