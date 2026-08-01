/**
 * Page object for the **Activities view** — the main-pane view an inactive session shows in place
 * of the terminal (its recorded ACP transcript, replayed from the daemon's persisted files).
 *
 * All raw selectors live here; test bodies call named methods. The transcript *body* is the shared
 * `AgentChatView`, so entry-level assertions belong to `agentChatPage` — this object covers the
 * pane that hosts it, its empty state, and the top-bar Resume that accompanies it.
 *
 * PRD: docs/ft/web/inactive-session-activities.md
 */

import { byTestId, sessionsMainResumeBtn, TEST_IDS } from "../testIds";
import { agentChatPage } from "./agentChatPage";

export const sessionActivitiesPage = {
  /** The Activities view container — present only when an inactive session shows it as its base view. */
  pane: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsActivitiesPane, { timeout: 5000, ...options }),

  /** The "no recorded activity" state, shown in place of the transcript for an empty session. */
  empty: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsActivitiesEmpty, { timeout: 5000, ...options }),

  /** The main pane's top-bar Resume button for `sessionId`. */
  resumeBtn: (sessionId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsMainResumeBtn(sessionId), { timeout: 5000, ...options }),

  /** Click the main pane's top-bar Resume button for `sessionId`. */
  resume(sessionId: string) {
    byTestId(sessionsMainResumeBtn(sessionId)).click();
  },

  /** Open the tool-call detail dialog for the transcript entry at `index` (arrival order). The
   *  transcript body is the shared `AgentChatView`, so entry-level assertions belong to
   *  `agentChatPage` — this is only the click that opens the dialog from *this* surface. */
  openToolDetail(index: number) {
    agentChatPage.chatMessage(index).click();
  },
};
