# 2026-10-17 — Host selection survives reload

**Type:** Fix

Reloading the dashboard while the common room was still joining moved the operator off their host.
The host directory lists the serving daemon first. The selection rule honoured `?host=` only when
its daemon was already listed, and the write-back then rewrote the URL to the serving daemon.

A chosen host (`?host=`, then the tab's stored selection) is now kept whether or not it is listed. The
selector shows an unlisted chosen host as waiting, under the label this tab last saw it listed
with. That label is stored as the protobuf JSON of a new TS-only `web_state.RememberedDaemonLabels`.

Packages:
- `tddy-web`: [entry](../../../packages/tddy-web/docs/changesets/2026-10-17-host-selection-survives-reload.md)
- `tddy-service`: `web_state.proto`, [entry](../../../packages/tddy-service/docs/changesets/2026-10-17-web-state-proto.md)

Product: [web changelog](../../ft/web/changelog/2026-10-17-host-selection-survives-reload.md).
No `docs/dev/todo/` entry was resolved. The one considered,
`2026-08-01-web-url-state-routing-follow-ups.md`, constrained where the tests could live and stays
open. No code issue was affected: the three `tddy-web` records are on files this change did not edit.
