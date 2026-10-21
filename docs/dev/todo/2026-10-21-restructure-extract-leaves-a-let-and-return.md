# 2026-10-21 — an extraction that ends a function with its own `let` leaves `let x = …; x` (clippy::let_and_return)

**Category:** Future enhancement (engine gap, rust-analyzer assist)
**Source:** `#host-desktops` 10/10, [#693](https://github.com/uppin/tddy-coder/pull/693) — plan `26-rdp-connect`

## What happened

Plan 26 extracts `connector_config` out of `RdpClient::connect_impl` (`packages/tddy-rdp/src/rdp_client.rs`). The
apply compiled, but `cargo clippy -p tddy-rdp --all-targets -- -D warnings` failed on the extracted function:

```
error: returning the result of a `let` binding from a block
   --> packages/tddy-rdp/src/rdp_client.rs:368:5
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.94.0/index.html#let_and_return
```

The range was the `let config = Config { … };` statement; the assist returns the bound name, producing
`let config = Config { … }; config`. `restructure check --deep` and the compile gate cannot see lints.

## Workaround used

Lint fix, developer-approved 2026-10-21 (two-line class): the literal is returned directly (`Config { … }`), one line
deleted and two edited, no behaviour change.

## Fix

When the extracted range is a single `let` whose value is returned, the engine should return the expression instead
of binding it first, or run clippy over the plan's packages after the compile gate.
