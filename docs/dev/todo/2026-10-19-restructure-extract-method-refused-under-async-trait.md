# 2026-10-19 — `extract_method` is refused inside an `#[async_trait]` impl

**Category:** Future enhancement (engine gap)
**Source:** `#host-desktops` 1/8, [#653](https://github.com/uppin/tddy-coder/pull/653) — seam C of the
screen-sharing service split

## What happened

`restructure check --deep` over four `extract_method` operations, one per host-target RPC body
(`list_host_targets`, `add_host_target`, `start_host_stream`, `stop_host_stream` in
`packages/tddy-screen-sharing/src/screen_sharing_service.rs`, each a statements-to-tail range, bottom-up),
refused every one:

```
this seam cannot be cut here: rust-analyzer offers no "extract into function" assist for the given range (it offered: Extract into variable)
```

A probe over a single plain `let` statement of `start_host_stream` (`let target = ...?;`) was refused the
same way, so the range shape (`?`, `.await`, a moved parameter) is not the cause.

## Cause: not confirmed

The methods sit in `#[async_trait] impl ScreenSharingService for ScreenSharingServiceImpl`. The suspicion is
that rust-analyzer offers no function extraction for code inside an attribute-macro-expanded item, because the
range does not map back through the expansion. That was not verified: no experiment was run with the attribute
removed, nor on another `#[async_trait]` impl.

## Consequence

Seam C (the bodies of the four host-target RPCs moved into a `host_targets` module behind one-line
delegators) was not done in #653. `screen_sharing_service.rs` stays at 537 production lines against a 500
budget.

## What would close it

- Confirm or refute the `#[async_trait]` cause, and if confirmed, have the engine offer `extract_method` for a
  method body of such an impl (or a dedicated operation that moves a trait-impl method body into an inherent
  method and leaves a delegator).
- Then plan seam C as `extract_method` to inherent `*_impl` methods followed by `extract_module`.
