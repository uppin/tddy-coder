# Create-session Provider field

Where a new CLI session's harness runs: its vendor, or one of the person's provider accounts.
Product view: [tool-session model selection § Provider](../../../docs/ft/web/tool-session-model-selection.md#provider).

## Pieces

| Item | Role |
|---|---|
| `components/sessions/CreateSessionProviderField.tsx` | `CreateSessionProviderField({ harnessRoutable, harnessReason, choices, value, onChange, error })`. `value` is `BUILT_IN_PROVIDER` (`"builtin"`) or an account id. Routable: a `<select>` whose first option is "Anthropic (built in)", then one option per `ProviderRouteChoice { accountId, label, preset, supported, reason }` — an unsupported one disabled, its reason appended to the label and set as `title`. Not routable: `harnessReason` as text instead of the select. `error` renders under the select |
| `rpc/useProviderRoutes.ts` | `useProviderRoutes(client, sessionToken, harness)` → `{ harnessRoutable, harnessReason, choices, error }` from `ListProviderRoutes`; idle (routable, no choices) without a client or harness; a failure is kept as `error`, never masked. `useProviderAccountModels(client, sessionToken, accountId)` → the `AgentModelsState` shape `useAgentModels` returns, from `ListProviderAccountModels` (`defaultModel` = the first id), so `CreateSessionModelField` renders either |
| `useProviderDefault(client, sessionToken, harness)` (in `rpc/useProviderRoutes.ts`) | the account whose `defaultFor` lists `harness`, from `ListAccounts`; `null` while unknown, when none is, or when the listing failed (the daemon still applies the default at start). `useCreateSessionModel` sets an untouched `providerChoice` to it once `ListProviderRoutes` lists it as supported |
| `createSessionPaneProps.ts` | `CreateSessionPaneProps.accountsClient?: Client<typeof AccountsService>`. Absent: no Provider field |
| `CreateSessionPane.tsx` | holds `providerChoice: string \| null` (`null` = untouched, reset on a session-type change); `providerHarness` is the session type for a CLI session, `null` for a tool session; the model list is `useProviderAccountModels` while an account is chosen, else `useAgentModels` |
| `createSessionRequest.ts` | `CreateSessionFormValues.providerAccountId?` / `providerBuiltIn?`, sent on the claude-cli and opencode-cli `StartSessionRequest`s only. `SessionType` is `"tool" \| "claude-cli" \| "cursor-cli" \| "opencode-cli"`; `daemonCutsTheWorktree(sessionType)` (every CLI session type) gates the **Create remote branch** toggle. The opencode-cli request sends the model and the provider choice and clears the first prompt, sandbox, managed codebase, specialized agents and semantic index |
| `CreateSessionTypeToggle.tsx` | the session-type buttons; **OpenCode** is `create-session-type-opencode-cli`. A CLI session type's model list is `ListAgentModels(<session type>)` — `modelAgentKey` is the session type for a CLI session, the chosen agent for a tool session |
| `SessionsDrawerScreen.tsx` → `SessionMainPane.tsx` | build `accountsClient` with `useDaemonClient(AccountsService)` and pass it to the create pane |

Test ids (`cypress/support/testIds.ts`): `create-session-provider-select`,
`create-session-provider-unavailable`, `create-session-provider-error`.

## Tests

`cypress/component/CreateSessionProviderAcceptance.cy.tsx` — defaults to built in; choosing an
account lists that account's models; Create sends the account and model; Cursor shows why; a
`ListProviderRoutes` failure is shown under the select.
`cypress/component/ProviderAccountDefaultsAcceptance.cy.tsx` — the person's claude-cli default is
pre-selected.

`cypress/component/CreateSessionOpenCodeAcceptance.cy.tsx` — the toggle offers OpenCode; its model
list is the daemon's `ListAgentModels("opencode-cli")`; Create sends an `opencode-cli` start with the
model; the Create remote branch toggle is offered unchecked, and when checked the start sends
`createRemoteBranch = true`.
