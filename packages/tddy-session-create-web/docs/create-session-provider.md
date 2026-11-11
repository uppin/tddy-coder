# Create-session Provider field

Where a new CLI session's harness runs: its vendor, or one of the person's provider accounts.
Product view: [tool-session model selection § Provider](../../../docs/ft/web/tool-session-model-selection.md#provider).

## Pieces

| Item | Role |
|---|---|
| `src/CreateSessionProviderField.tsx` | `CreateSessionProviderField({ harnessRoutable, harnessReason, choices, value, onChange, error })`. `value` is `BUILT_IN_PROVIDER` (`"builtin"`) or an account id. Routable: a `<select>` whose first option is "Anthropic (built in)", then one option per `ProviderRouteChoice { accountId, label, preset, supported, reason }` — an unsupported one disabled, its reason appended to the label and set as `title`. Not routable: `harnessReason` as text instead of the select. `error` renders under the select |
| `src/useProviderRoutes.ts` | `useProviderRoutes(client, sessionToken, harness)` → `{ harnessRoutable, harnessReason, choices, error }` from `ListProviderRoutes`; idle (routable, no choices) without a client or harness; a failure is kept as `error`, never masked. `useProviderAccountModels(client, sessionToken, accountId)` → the `AgentModelsState` shape `useAgentModels` returns, from `ListProviderAccountModels` (`defaultModel` = the first id), so `CreateSessionModelField` renders either |
| `useProviderDefault(client, sessionToken, harness)` (in `src/useProviderRoutes.ts`) | the account whose `defaultFor` lists `harness`, from `ListAccounts`; `null` while unknown, when none is, or when the listing failed (the daemon still applies the default at start). `useCreateSessionModel` sets an untouched `providerChoice` to it once `ListProviderRoutes` lists it as supported |
| `createSessionPaneProps.ts` | `CreateSessionPaneProps.accountsClient?: Client<typeof AccountsService>`. Absent: no Provider field |
| `CreateSessionPane.tsx` | holds `providerChoice: string \| null` (`null` = untouched, reset on a session-type change); `providerHarness` (`providerHarnessFor(sessionType, runsThePickedAgent, selectedAgentId, routeProviders)` in `useCreateSessionModel.ts`; `runsThePickedAgent` is a session type served with the `tddy_coder` launch) is the chosen agent for a tddy-coder session, `null` when the served agent declares no route providers (so no Provider field), and the session type for every other CLI session; the model list is `useProviderAccountModels` while an account is chosen, else `useAgentModels` |
| `createSessionRequest.ts` | `CreateSessionFormValues.providerAccountId?` / `providerBuiltIn?`, sent on a CLI `StartSessionRequest` when the agent's spec binds the fields (`CliAgentFeatures`, derived from the served agent's `run_params` and `capabilities`). `SessionType` is a plain `string` — the bundle names no session type but `NO_AGENT_SESSION_TYPE`; `CreateSessionFormValues.sessionLaunch` (the served launch kind) decides the request's shape, and the chosen session type is always sent (the daemon refuses an empty one); `daemonCutsTheWorktree(sessionLaunch)` (every launch but `tddy_coder`) gates the **Create remote branch** toggle |
| `CreateSessionTypeToggle.tsx` | the session-type buttons, one per session type the target daemon declares (`create-session-type-<session type>`), from `ListAgentsResponse.session_types` through `sessionTypeOptions` (`cliHarnessOptions.ts`): those with the `tddy_coder` launch first, then the coding agents' CLIs, labelled as served; the form opens on the first. Every decision reads the served launch kind (`sessionLaunchOf`), never the session type's spelling. Which fields mount is the served agent's metadata: the model field when its argv binds `model`, the provider field when it declares route providers. A CLI session type's model list is `ListAgentModels(<session type>)` — `modelAgentKey` is the session type for a CLI session, the chosen agent for a tddy-coder session |
| `tddy-web`: `useSessionsDrawerClients.ts` → `SessionsDrawerScreen.tsx` → `SessionMainPane.tsx` | build `accountsClient` with `useDaemonClient(AccountsService)` and pass it to the create pane |

Test ids (`cypress/support/testIds.ts`): `create-session-provider-select`,
`create-session-provider-unavailable`, `create-session-provider-error`.

## Tests

`cypress/component/CreateSessionProviderAcceptance.cy.tsx` — defaults to built in; choosing an
account lists that account's models; Create sends the account and model; Cursor shows why; a
`ListProviderRoutes` failure is shown under the select.
`tddy-web`'s `cypress/component/ProviderAccountDefaultsAcceptance.cy.tsx` — the person's claude-cli and muse-cli
defaults are pre-selected.

`cypress/component/CreateSessionOpenCodeAcceptance.cy.tsx` — the toggle offers OpenCode; its model
list is the daemon's `ListAgentModels("opencode-cli")`; Create sends an `opencode-cli` start with the
model; the Create remote branch toggle is offered unchecked, and when checked the start sends
`createRemoteBranch = true`.

`cypress/component/CreateSessionMuseAcceptance.cy.tsx` — the toggle offers Muse Code; Create sends a
`muse-cli` start with the chosen Muse model.

`cypress/component/CreateSessionWarpAcceptance.cy.tsx` — Create starts a `warp-cli` session with no
model chosen (`model: ""`); the form offers no model or provider field for Warp.
