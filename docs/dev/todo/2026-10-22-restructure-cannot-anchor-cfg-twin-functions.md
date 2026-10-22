# 2026-10-22 — `restructure` cannot anchor a function that has a `cfg` twin of the same name

**Deferred (consent):** 2026-10-22 — user: "defer the remaining", given during `#satellite` 16/16, the sweep ([#695](https://github.com/uppin/tddy-coder/pull/695)). Not taken by the stack; stays open.
**Category:** Future enhancement (engine gap)
**Source:** `#satellite` 16/16 (the sweep, [#695](https://github.com/uppin/tddy-coder/pull/695)),
`packages/tddy-tools/src/cli.rs`: a probe plan (`extract_module` over the relay families' line range),
recorded in the sweep's change-history entry
[`2026-10-22-satellite-16-sweep.md`](../changesets/2026-10-22-satellite-16-sweep.md)

`cli.rs` holds five families of `relay_*` functions, each a `#[cfg(unix)]` / `#[cfg(not(unix))]` pair of
**same-named** functions. rust-analyzer analyses one `cfg` arm, so the engine cannot name the pair:

- an `items` anchor is refused (`names 2 items … qualify it with the trait`; `#N` is accepted only for
  inherent impls);
- `--at` on the run is refused (`inside no item`);
- a range `extract_module` over the families is refused (`no import rust-analyzer offered for \`args\``: the
  inactive arm is not analysed).

The sweep moved the five families (231 lines) into `cli/relay.rs` by hand, bodies byte-identical, with the
developer's consent (D6). The same shape is `this_process_os_user` in `tddy-daemon/src/runtime.rs`, which
stays where it is.

**What would close it:** an item-anchor spelling for `cfg` twins (for example `relay_ask@cfg(unix)`), resolved
by reading the item's attribute rather than through the outline, so `move_item` can take a pair together.

**Marked in code:** `TODO(restructure-engine)` at the head of `packages/tddy-tools/src/cli/relay.rs`.
