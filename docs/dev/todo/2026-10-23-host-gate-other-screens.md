# 2026-10-23 — Accounts, Worktrees and VMs are not behind the host connection gate

**Category:** Deferred from `web-dev` LiveKit and visible errors (#706)
**Source:** `packages/tddy-web/src/components/shell/AppShell.tsx` (`requiresHost`), `HostConnectionGate.tsx`

## What happens

`AppShell`'s `requiresHost` prop blocks a screen's body while no usable RPC client reaches the selected host, and
says why (`HostConnectionGate`, decided by `hostGate.ts`). #706 turns it on for **Projects** and **Sessions** only.
`AccountsAppPage`, `WorktreesAppPage` and `VmsAppPage` are also scoped to the selected host (they call
`useDaemonClient`), but they still render their controls when nothing reaches it. VMs still swallows its RPC
failures (`.catch(() => {})`).

## Why deferred

The developer limited #706 to Projects and sessions. Each of those screens would need its own Cypress pass, because
many specs mount them with fixtures that may never reach the host.

## What would close it

Add `requiresHost` to each of the three screens. Run the specs that mount them, and fix any fixture that never
reaches the host (`aJoinedCommonRoom` in `cypress/support/rpc/withSelectedDaemon.tsx`). For VMs, also convert its
silent catches to inline errors with `useActionFailure`.
