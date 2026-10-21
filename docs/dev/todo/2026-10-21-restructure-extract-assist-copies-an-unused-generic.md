# 2026-10-21 — the extract-into-function assist copies the host's unused type parameter (E0283)

**Category:** Future enhancement (engine gap, rust-analyzer assist)
**Source:** `#host-desktops` 10/10, [#693](https://github.com/uppin/tddy-coder/pull/693) — plan `17-bridge-start-bridge`

## What happened

Plan 17 extracts `publish_to_livekit` out of `start_bridge<C: ScreenSharingClient>`
(`packages/tddy-screenshare/src/bridge.rs`). `check --deep` said `no findings`; the apply's compile gate failed:

```
packages/tddy-screenshare/src/bridge.rs:265:32: error[E0283]: type annotations needed: cannot infer type of the type parameter `C` declared on the function `publish_to_livekit`
```

The assist declared `async fn publish_to_livekit<C: ScreenSharingClient>(config: &BridgeConfig, width: u32, height: u32)`
although the extracted lines never mention `C`, so no call can infer it.

## Workaround used

Two steps, both developer-approved (2026-10-21):

1. The call was written `publish_to_livekit::<C>(...)` with a `TODO(host-desktops sweep)` comment, so the plan's
   apply could compile (`apply --resume` applied nothing, so `cargo fix` and `cargo fmt -p tddy-screenshare`
   finished it).
2. `cargo clippy -p tddy-screenshare --all-targets -- -D warnings` then failed with
   `error: type parameter C goes unused in function definition` (`clippy::extra_unused_type_parameters`), so the
   generic was **deleted by hand**: `<C: ScreenSharingClient>` is gone from `publish_to_livekit` and so are the
   `::<C>` turbofish and its TODO at the call (no `#[allow]`). `start_bridge` keeps its own `C`.

## Fix

The engine (or the assist) should drop type parameters the extracted range does not use, so no hand edit is
needed: the extraction should compile and lint clean as written.
