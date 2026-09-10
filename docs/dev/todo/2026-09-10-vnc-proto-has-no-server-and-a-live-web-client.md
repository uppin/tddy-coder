# 2026-09-10 — `vnc.proto` has no server, and the web still calls it

**Category:** Future enhancement
**Source:** `#unbundle` node 2 (PR [#471](https://github.com/uppin/tddy-coder/pull/471)) —
[the changeset](../changesets/2026-09-09-unbundle-model-telegram-screen.md)

Node 2 deleted `vnc_service.rs`, `vnc_vault.rs` and their two acceptance suites: 1,008 lines that
were compiled, tested and reachable by nothing. `runtime.rs` never registered `vnc.VncService`, and
`packages/tddy-daemon/tests/service_registration_acceptance.rs` now pins that absence against the
live registry so it cannot regress into a re-registration.

**The schema was deliberately left in place.** Removing a server is provable from `runtime.rs`;
removing a schema is a judgement about whether anyone intends to revive it, and that was not node
2's call. This entry is the decision, deferred.

## What is still standing

| Artefact | State |
|---|---|
| `packages/tddy-service/proto/vnc.proto` | 6 rpcs, compiled by `build.rs:276` and `:434` |
| `packages/tddy-service/src/lib.rs:137` `pub mod vnc` | generated Rust types, no implementor |
| `packages/tddy-web/src/gen/vnc_pb.ts`, `packages/tddy-rust-typescript-tests/gen/vnc_pb.ts` | generated TS clients, committed |
| `packages/tddy-web/src/components/sessions/SessionInspectorDrawer.tsx:6,165` | **a live client** — `useHttpClient(VncService)` |
| `SessionVncTab.tsx`, `VncOverlay.tsx`, `InspectorTabs.tsx:102` | a **user-reachable "vnc" tab** in the session inspector |
| 4 Cypress component specs + `cypress/support/rpc/{vncRpcs,vncBackend}.ts` | exercise that tab against fakes |

So the deletion did not create a dangling client — it **revealed** one. The web's VNC tab has been
dialling a service no daemon registers; every call it makes fails, and the Cypress specs pass
because they answer it with a fake backend rather than a daemon. The live desktop path is
`screen_sharing.ScreenSharingService` (`SessionScreenSharingTab`, in the same drawer).

**`vnc_input.proto` is a different thing and must not be swept up.** `VncInputService` is served by
the `tddy-vnc` bridge process for input forwarding and is live.

## The decision to make

1. **Retire it** — delete `vnc.proto`, its `build.rs` entries, `tddy_service::proto::vnc`, both
   generated `vnc_pb.ts`, the inspector's VNC tab and its four Cypress specs. This is the honest
   option if screen sharing is the desktop surface, but it is a **product-visible removal of a UI
   tab**, so it needs a product call and belongs in its own PR — not in a relocation node.
2. **Keep it** — then the tab needs a server, and someone has to say which process serves it.

Either way, a generated client with no server in a shipping UI should not survive another release
undecided.

## Why it was deferred

Node 2's boundary is "three subsystems leave `tddy-daemon`, and one dead one is deleted". Deleting
a proto changes `tddy-service` and regenerates two committed `gen/` directories under the CI drift
gate; removing the tab changes `tddy-web`. Both are outside a node whose reviewability rests on the
diff being import re-points.
