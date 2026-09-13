# 2026-09-12 — `generate_tonic_adapter` hardcodes `tddy_service::to_tonic_status`, so `tddy-service` had to alias itself

**Category:** Future enhancement
**Source:** `#unbundle` node 7, [#476](https://github.com/uppin/tddy-coder/pull/476), reporting
upward to node 6, [#475](https://github.com/uppin/tddy-coder/pull/475)

Node 6's `generate_tonic_adapter` emits handler bodies that call `tddy_service::to_tonic_status`, by
design: one shared conversion means a refusal cannot reach two transports as two different gRPC
codes, and [`packages/tddy-codegen/docs/tonic-adapter.md`](../../../packages/tddy-codegen/docs/tonic-adapter.md)
calls the path *deliberately not configurable*.

Node 7's two adapters are the first generated ones to land in **`tddy-service`'s own** `OUT_DIR`.
Inside that crate `tddy_service::…` does not resolve — a crate cannot name itself by name — so the
generated code does not compile as emitted.

The fix applied was one line, `packages/tddy-service/src/lib.rs:11`:

```rust
extern crate self as tddy_service;
```

It works, and it is the idiomatic workaround. It is recorded because it is a **generator** property
that every future in-crate adapter will meet, and because the line reads as unexplained if you have
not met the generator: nothing at the call site says why a crate aliases itself.

Node 7 did not change the generator, per its `## Boundaries`: a gap found here is reported to node 6
rather than fixed in a node that does not own `tddy-codegen`.

## What closing it would take

Either a `status_path` config field on the generator's options, defaulting to
`tddy_service::to_tonic_status` and set to `crate::to_tonic_status` for a pass emitting into
`tddy-service` — or, if the alias stays, a comment on `lib.rs:11` naming the generator, so the next
reader does not delete a line that looks vestigial.
