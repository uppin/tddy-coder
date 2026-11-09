# 2026-11-07 — Agent chat moves here (#web-split 10/17, PR #847)

New package. From `tddy-web`'s `components/chat/`: `AgentChat`, `LiveAgentChatView`,
`AgentTranscriptView`, `useAgentChat`, the ACP hooks and presentation helpers. From
`components/sessions/`: `agentActivityRegistry`, the activity panes, the subagent conversation and
permission UI, `pendingPermission`, `useAnswerAgentPermission`, `useSessionActivity`,
`sessionRosterHalf`; from `lib/`: `scrollFollow`. 13 Cypress specs and 7 `bun test` suites moved with
them; specs mount with `cy.mountWithClients` (live chat through `mountWithChatRoom`).

At wrap:
- `useAcpSession.ts` split: handshake, prompt and permission-reply frames and the
  `request_permission` → question mapping moved to `acpSessionMessages.ts` (mechanical, same
  behaviour). Final measurement for the closed `oversized-file-use-acp-session` record:
  **536 → 522 → 449 lines** (budget 500); `acpSessionMessages.ts` is 107.
- `chatTranscript`'s role map named no `subagent`, so an exported transcript printed `undefined` for a
  subagent bubble — mapped, with a unit test.
- Stream errors log through `tddyDebug` (`tddy:acp:session`, `tddy:acp:replay`,
  `tddy:presenter:stream`) instead of `console.debug`.
- The package typechecks clean: the elicitation specs read sent intents through `theOnlyIntent`
  (typed per intent kind) and build options with `create(QuestionOptionProtoSchema, …)`.
