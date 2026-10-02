# 2026-10-02 — A static `check` cannot verify item anchors, only decline to

**Category:** Future enhancement
**Source:** `/pr-wrap` validation of #537 (`#live-plan` 1/7)

`restructure check` without `--deep` reads text and starts no server. Resolving an item anchor needs
rust-analyzer's outline, so `unresolvable_without_a_server`
(`packages/tddy-code-restructuring/src/runner/entry_points.rs`) reports each item-anchored operation as
a finding saying to run `check --deep`. A plan of item anchors therefore never passes a static check
green, and a gate that runs only the static form cannot say anything about such a plan.

A static check could parse the item path and verify what needs no server: the crate and module prefix
against the file (`module_path_of`), and the relative range against the item path's shape. It could
not verify presence, ambiguity or the fingerprint.

**Why deferred.** The honest answer for the parts that need a server is the finding already there; a
partial static verdict risks reading as "checked". Worth revisiting if a CI gate wants a cheap pre-pass.
