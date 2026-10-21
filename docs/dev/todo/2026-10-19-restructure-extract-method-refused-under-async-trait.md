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

## Cause: narrowed by probes (2026-10-19), mechanism still not read from rust-analyzer

`check --deep` over single-operation `extract_method` plans on the same file and the same warm index:

| Probe | Where | Result |
|---|---|---|
| P1 | two statements of the plain inherent `resolve_session_dir` | no findings |
| P2 | a `let ...?;` statement of the inherent `async fn spawn_bridge` (`bridge_lifecycle.rs`) | no findings |
| P3 | one `self.require_user(...)?;` statement of `stop_host_stream` (`#[async_trait]` impl) | refused: offers only "Extract into variable" |
| P4 | the tail expression of `stop_host_stream` | refused: no hover at the position, "cannot type this position" |
| P5 | P3 with `variant: "module"` | refused, same as P3 |

So `async fn`, `?`, `.await` and moved parameters are not the cause; the `#[async_trait]` impl is. Inside it
rust-analyzer either cannot type positions (P4) or offers no function extraction (P3, P5), consistent with
the bodies being attribute-macro-expanded. The expansion itself was not inspected.

No other operation in `plan-schema.md` reaches a trait-impl method body: `retarget_impl` refuses a trait impl at
parse time, `extract_module` over some members of a trait impl would produce a second `impl Trait for Type`
(`E0119`/`E0046`, documented), and the rest act on inherent items or ranges.

## Consequence

Seam C of #653 (the bodies of the four host-target RPCs into a `host_targets` module behind delegators) could not
be planned. With the developer's consent on 2026-10-19 it was **hand-moved** instead: the four bodies (117 lines,
verified byte-identical by script) now live in inherent `*_impl` methods in
`packages/tddy-screen-sharing/src/screen_sharing_service/host_targets.rs`, and the trait methods delegate.
`screen_sharing_service.rs` is at 421 production lines. **The engine gap stays open**: the next trait-impl body
under `#[async_trait]` that needs the same move will hit the same refusal.

## What would close it

- Confirm or refute the `#[async_trait]` cause, and if confirmed, have the engine offer `extract_method` for a
  method body of such an impl (or a dedicated operation that moves a trait-impl method body into an inherent
  method and leaves a delegator).
- Then plan seam C as `extract_method` to inherent `*_impl` methods followed by `extract_module`.

## Reconfirmed (2026-10-21, `#host-desktops` 10/10, [#693](https://github.com/uppin/tddy-coder/pull/693))

Three more `check --deep` probes, one per `#[async_trait]` impl the sweep needed to shorten, all refused with the
same text (`this seam cannot be cut here: rust-analyzer offers no "extract into function" assist for the given range
(it offered: Extract into variable)`):

| Probe | Where (on `master` before the sweep) |
|---|---|
| a | `ScreenSharingService::start_stream`, the bridge-start block (`tddy-screen-sharing/src/screen_sharing_service.rs:416–449`) |
| b | `ScreenSharingClient for RdpClient::connect`, the `Config { … }` literal (`tddy-rdp/src/rdp_client.rs:155–198`) |
| c | `HostService::add_host_key`, two `let` statements (`tddy-host-service/src/service.rs:683–692`) |

The gap was worked around the same way as #653's seam C, with the developer's consent: eleven trait bodies
(`HostService`'s nine, `start_stream`, `RdpClient::connect`) were moved by hand, byte for byte (proved by script),
into inherent `*_impl` methods behind one-line delegators, and the engine then extracted modules and helpers from
the inherent methods, where the same ranges were accepted. The gap stays open.
