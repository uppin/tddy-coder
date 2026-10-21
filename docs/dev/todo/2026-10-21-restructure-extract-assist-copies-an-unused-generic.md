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

Developer-approved one-token post-move build fix (2026-10-21): the call became `publish_to_livekit::<C>(...)`, with
a `TODO(host-desktops sweep)` comment on it. `apply --resume` applied nothing, so `cargo fix` and `cargo fmt -p
tddy-screenshare` finished the plan.

## Fix

The engine (or the assist) should drop type parameters the extracted range does not use. Then delete `<C: ScreenSharingClient>`
on `publish_to_livekit` and the turbofish with its TODO.
