# Operator-visible failures

A failed RPC is shown next to the control or section that caused it, in the daemon's own words.
Nothing an operator triggered or is waiting on is swallowed. Two failures that look alike to a
screen are kept apart:

- **The daemon refused the call.** Its reason is shown verbatim.
- **Nothing reaches the host.** `useDaemonClient` answered `null`, so the request was never sent.
  This is raised as an error of its own, naming the host, never handled with a silent `return`.

The layout handles the second case wholesale for host-scoped screens: see
[host-connections.md § The host connection gate](host-connections.md#the-host-connection-gate).

## Vocabulary — `src/rpc/rpcFailure.ts`

| Export | Use |
|---|---|
| `rpcFailureText(error)` | The text to show for any failure: `ConnectError.rawMessage` without the transport's `[code]` prefix, with `PermissionDenied` called out |
| `hostNotConnected(host)` | The `ConnectError` (`Unavailable`) an action raises when it has no client: "Not connected to `<host>` — it may be offline or not yet in the room." |
| `hostNotConnectedText(host)` | The same, already as text, for a surface that holds a string |
| `actionFailed(action, error)` | `Delete failed: <reason>`, converted once |

Hosts are named by `useHostLabel()` (`src/rpc/selectedDaemon.tsx`, over the pure `hostLabelOf` in
`src/rpc/hostLabel.ts`). It is one rule everywhere a host appears in text: the listed label, with the
self-advertised ` (this daemon)` suffix dropped (`SELF_ADVERTISED_LABEL_SUFFIX`), else the instance
id. The callback changes identity exactly when a name it can give changes, and not when the daemon
list is merely rebuilt with the same names. A poll keyed on it is therefore not restarted by every
participant event, and a compiled caller still recomputes a label that changed
(`cypress/component/HostLabelIdentityAcceptance.cy.tsx`).

## Rendering and state

- **`components/ui/inline-error.tsx` — `<InlineError testId message />`.** Renders nothing without a
  message. Otherwise it renders `role="alert"` and `text-destructive`.
- **`hooks/useActionFailure.ts` — one surface's failure slot: `{ error, pending, run, report, clear }`.**
  - `run(action)` clears the previous failure, records this one, and never rejects, so a caller has
    nothing left to swallow.
  - `pending` disables a button while its action is in flight ("Creating…", "Adding…").
- **`hooks/useKeyedActionFailure.ts` — the same, per row: `{ errors, run(key, action), clear(key) }`.**
  Used by the VNC, screen-sharing and files tabs.

Rules the screens follow:

- A **container's action props return promises** that reject with the reason. The presentational
  screen awaits them and renders the reason where the action was taken. A form keeps what was typed
  until the action succeeds.
- A **failed load is never shown as an empty list.** The error is rendered beside the last good data,
  and it clears on the next successful load.
- A **stream failure** shows a quiet "`<what>` unavailable: `<reason>`" in the surface that owns it,
  until data flows again. The streams do not reconnect by themselves; see the backlog.
- **Deliberately silent**, each with a comment saying why:
  - a fullscreen request the browser refuses;
  - closing a stream whose surface is already closing;
  - the background token refresh, where logout is reported elsewhere;
  - parse and storage guards.

## Where it applies

- **Projects:** see [projects-screen.md § Failures](projects-screen.md#failures).
- **Session inspector tabs** (VNC, screen sharing, files, worktree, tools, terminals): the list
  error, and per-row errors for add, start, remove and delete.
- **Sessions drawer and runtime:**
  - list load, resume, delete and terminate;
  - bulk delete, as one line per failed row, where an unreachable owner fails its row and is never
    routed to another daemon;
  - child-runtime attach and terminal claim.
- **Terminal input:** a single "Input not delivered" indicator. `rpc/connections/inputDelivery.ts`
  numbers each send, so a late refusal cannot overwrite a later success.
- **Create-session form:** the projects, tools, stack-parent and branch pickers.
- **Session streams:** usage and notifications. Usage reports nothing while there is no room or the
  room is still joining.

Acceptance:
- `cypress/component/ProjectsVisibleErrorsAcceptance.cy.tsx`
- `cypress/component/SessionTabsVisibleErrorsAcceptance.cy.tsx`
- `cypress/component/SessionsDrawerVisibleErrorsAcceptance.cy.tsx`
- `cypress/component/CreateSessionAndStreamsVisibleErrorsAcceptance.cy.tsx`
