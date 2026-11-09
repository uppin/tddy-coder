# 2026-11-07 — The sessions drawer, project home and status strip move here (#web-split 18/20, PR #862)

The rest of `tddy-web`'s `components/sessions/` moved here: the drawer screen and its hooks, the drawer
list, project home, the session list store and drawer state, the status bar, traffic strip, host stats
footer and upload progress, `vncInput`. A new seam, `SessionTrafficProvider` / `useSessionTraffic`
(the meter registry and the LiveKit room a connection runs over), is filled by the app; the drawer
reads auth through `useAuth`. The drawer-driven specs other nodes left behind (PR-stack, create-session,
host, provider defaults) moved with their support, rewritten on the port doubles — 136 specs in the
package. `bunfig.toml` and `dom-preload.ts` moved here.

At wrap: `src/` typechecks clean — `sessionManager` types its interval id as the browser's `number`, and
three suites' session builders take message-init overrides (the metadata fixture names every field).
The moved specs' 181 fixture errors are recorded in `docs/dev/todo/2026-11-07-session-web-specs-do-not-typecheck.md`.
