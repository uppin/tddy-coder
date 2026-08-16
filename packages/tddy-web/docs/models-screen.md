# Models & Agents screen (`#/models`)

Lists the **providers**, **models** and **assistants** of every connected daemon in one place, with
per-row load/unload, chat, and assistant composition.

Feature doc: [Models & Agents](../../../docs/ft/web/models-and-agents.md).

## Layout

Container/presentational split, matching `components/projects/`:

| File | Role |
|---|---|
| `ModelsAppPage.tsx` | Container — `AppShell`, chat/workspace dialog state, owns the fan-out hook |
| `ModelsScreen.tsx` | Pure presentational |
| `ProvidersPanel.tsx` / `AddProviderForm.tsx` | Provider rows, add form, delete |
| `ModelsTable.tsx` | Merged cross-daemon table, per-row actions, stale marking |
| `AssistantsPanel.tsx` / `CreateAssistantDialog.tsx` / `EditAssistantDialog.tsx` | Assistant list and composition |
| `AssistantToolPicker.tsx` | Tool checkboxes from the daemon's catalog |
| `ChatWorkspaceDialog.tsx` | Where a tool-bearing assistant's tools run |
| `ModelChatDialog.tsx` / `ModelsDialogShell.tsx` | Chat pane and shared modal chrome |
| `useModelRegistryFanOut.ts` | One client per daemon, merge, all mutations |
| `../../utils/mergeRegistryEntries.ts` | Pure merge, row keys, read status |
| `../../utils/registryChatTarget.ts` | Chat target (model vs assistant) and whether it needs a workspace |

Pure logic lives in `.ts` beside the `.tsx` so `bun test` runs it without the JSX runtime — the
established convention here.

## Cross-daemon fan-out

Routing follows [daemon-selector-livekit-rpc](../../../docs/ft/web/daemon-selector-livekit-rpc.md):
one `ModelRegistryService` client per common-room daemon, built the way `useDaemonClientFor` builds
one internally. It is **not** that hook, because a hook per daemon would change the hook count between
renders when the daemon list moves — the same reason `ProjectsAppPage` has `clientForHost`.

Every per-row action routes to the row's **owning** daemon, not the selected one.

**Degradation is per list.** The four reads per daemon go through `Promise.allSettled`, so a failed
assistant read still leaves that daemon's models visible. An unreachable daemon becomes one error row,
never an empty page. `registryReadStatus` distinguishes **not-connected / loading / read-failed /
empty**, so a first-run daemon with no providers does not look identical to a broken one.

**Keys are daemon-qualified.** Provider ids and assistant names are unique only *per daemon*
(`prov-ollama` exists on every host), so `providerRowKey`, `modelRowKey` and `assistantRowKey` all
compose `daemonInstanceId` with the row id — for React keys, `data-testid`s and the error maps alike.
Keying an error map by the bare id renders one daemon's failure against another's row.

## Chat

Reuses `useAcpSessionOverClient` over `AcpService` — there is no second chat transport. The target is
a `RegistryChatTarget`: `providerId`+`modelId` for a model, or `assistantId` alone for an assistant,
letting the daemon's own record decide provider, model, prompt and tools.

A **tool-bearing** assistant needs a workspace, so `ChatWorkspaceDialog` runs first. Its choices come
from `ListProjects` with `local_only: true` on the assistant's owning daemon — the same file the
daemon confines against, read through the same resolver, so every offered path is one the daemon will
accept rather than a free-text box the operator guesses into. `local_only` is load-bearing: a
fanned-out list also returns peers' rows, whose paths exist on other hosts and would be refused here.
A tool-less assistant, and any model, chats immediately with `cwd: ""`.

`toolCallUpdate` folds into the bubble its `toolCall` announced, matched by tool-call id, and renders
status plus result text. The mapping lives in `chat/toolCallPresentation.ts`, shared with
`acpReplayProjection`, so live and recorded transcripts cannot drift.

## Conventions

- Chat requires a **positive `llm` label**. A model the daemon labelled `unknown` gets no Chat button —
  the web does not guess what the daemon declined to determine.
- Unrecognised `ProviderKind` / `ModelLoadState` values render the raw enum value with a skew message,
  never "Unknown" — a version skew must not read as ordinary data.
- Failed RPCs surface visibly; `PERMISSION_DENIED` is prefixed so owner-only writes explain themselves.
- Both submit forms guard against double-firing.
- Dialogs dismiss on Escape and backdrop and set `aria-modal`. **Focus is not trapped** — no dialog in
  `tddy-web` does, so it belongs in shared chrome rather than here alone (`ModelsDialogShell.tsx:12`).

## Tests

`cypress/component/models/` — 84 specs across 14 files, driven by
`cypress/support/rpc/modelRegistryBackend.ts` (a stateful in-memory fake, not stubs) and
`mountWithPerDaemonLiveKitRpc` for the cross-host cases. Page object:
`cypress/support/pages/modelsScreenPage.ts`; its attribute accessors throw on a missing `data-*`
rather than substituting a default, so a component that stops emitting one fails loudly.

Pure logic: `mergeRegistryEntries.test.ts`, `registryChatTarget.test.ts`,
`chat/toolCallPresentation.test.ts` under `bun test`.
