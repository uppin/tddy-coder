# 2026-10-23 — Hosts reachable under the React Compiler; failures shown where they happen; the host connection gate

**Type:** Fix

Cross-package entry: [2026-10-23-web-dev-livekit-and-visible-failures.md](../../../../docs/dev/changesets/2026-10-23-web-dev-livekit-and-visible-failures.md).

- **`"use no memo"`** opts two functions out of the compiler. Under the compiler they kept a routing
  answer from before the common room joined, which left every `useDaemonClient` at `null`.
  - `useHostConnector` (`rpc/connections/registry.tsx`)
  - `SelectedDaemonScope` (`rpc/selectedDaemon.tsx`)

  See [host-connections.md § Under the React Compiler](../host-connections.md#under-the-react-compiler).
- **Failure vocabulary and slots:**
  - `rpc/rpcFailure.ts` (`rpcFailureText`, `hostNotConnected`, `hostNotConnectedText`, `actionFailed`)
  - `rpc/hostLabel.ts` and `useHostLabel`
  - `components/ui/inline-error.tsx`
  - `hooks/useActionFailure.ts` and `hooks/useKeyedActionFailure.ts`

  See [operator-visible-failures.md](../operator-visible-failures.md).
- **Inline failures** on Projects, the session inspector tabs, the sessions drawer and runtime,
  terminal claim and input (`rpc/connections/inputDelivery.ts`), the create-session pickers and the
  usage and notification streams.
  - Attach, delete and signal errors carry no transport `[code]` prefix.
  - Bulk delete fails a row whose owner is unreachable rather than routing it to another daemon.
- **Host connection gate:** `AppShell requiresHost` with `components/shell/HostConnectionGate.tsx`
  and `hostGate.ts`, opted into by Projects and Sessions.
- **Splits under the 500-line budget**, moves only:
  - `SessionRuntime.tsx`: 440 lines, with `SessionChildRuntime.tsx`, `sessionRuntimeProps.ts`,
    `useSessionPaneFullscreen.ts` and `useAgentTerminalFocus.ts` split out
  - `SessionsDrawerScreen.tsx`: 483 lines, with `useSessionsDrawerPaneActions.ts` and
    `SessionsDrawerNotices.tsx` split out
  - `CreateSessionPane.tsx`: 484 lines, with `useCreateSessionAgentSelection.ts` split out
  - `ProjectsScreen.tsx`: 221 lines, with `ProjectCard.tsx` (323) split out
  - `SessionDrawer.tsx`: 335 lines, with `SessionDrawerGroups.tsx` (253) split out
- **Closed code issues**, re-measured at wrap with `wc -l`:
  - `oversized-file-create-session-pane`: 484
  - `oversized-file-session-runtime`: 440
  - `oversized-file-session-drawer`: 335, from 567 when `#project-home` 2/7 detected it
  - `oversized-file-sessions-drawer-screen`: 498, from 511 when `#project-home` 4/7 detected it
- **`test:unit`** also covers `src/components/shell`.
