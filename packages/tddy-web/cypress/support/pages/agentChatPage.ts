/**
 * Page object for the reusable `AgentChat` component acceptance tests.
 *
 * All raw selectors live here; test bodies call named methods. No raw `cy.get(...)`
 * in test files — only these named helpers. Mirrors `prStackScreenPage`'s chat section
 * but over the recipe-agnostic `agent-chat-*` ids.
 */

import {
  byTestId,
  agentChatMessage,
  agentChatOption,
  agentChatMultiSelectOption,
  agentChatElapsed,
  agentChatToolStatus,
  TEST_IDS,
} from "../testIds";

export const agentChatPage = {
  /** The chat panel root. */
  chat: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentChat, { timeout: 5000, ...options }),

  /** The scrollable message list. */
  chatMessages: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentChatMessages, { timeout: 5000, ...options }),

  /** A single rendered chat bubble, in arrival order (0-indexed). */
  chatMessage: (index: number, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(agentChatMessage(index), { timeout: 5000, ...options }),

  /** The bubble kind ("user" | "agent" | "goal" | "activity" | "tool") of a chat bubble, in arrival order. */
  chatMessageKind: (index: number, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(agentChatMessage(index), { timeout: 5000, ...options }).invoke("attr", "data-message-kind"),

  /** The DEBUG-style "+Ns" elapsed badge on a read-only transcript entry, in arrival order. */
  chatElapsed: (index: number, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(agentChatElapsed(index), { timeout: 5000, ...options }),

  /** The status marker (running/error) on a read-only transcript tool-call entry, in arrival order. */
  chatToolStatus: (index: number, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(agentChatToolStatus(index), { timeout: 5000, ...options }),

  // ---------------------------------------------------------------------------
  // Read-only transcript: tail-first open, auto-follow, backwards paging
  //
  // Viewport facts are read off the component's hidden `agent-chat-scroll-state` mirror rather than
  // measured in the spec — one declared source of truth, so a layout change cannot quietly turn a
  // scroll assertion green (the same contract `terminal-page-scrollbar` provides for the terminal).
  // ---------------------------------------------------------------------------

  /** The hidden viewport mirror: `data-pinned`, `data-scroll-top`, `data-scroll-height`,
   *  `data-client-height`. */
  chatScrollState: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentChatScrollState, { timeout: 5000, ...options }),

  /** Assert the transcript is following the newest entry — new frames scroll into view. */
  expectFollowingNewest() {
    agentChatPage.chatScrollState().should("have.attr", "data-pinned", "true");
  },

  /** Assert the reader has detached from the newest entry — arriving frames leave the offset alone. */
  expectDetachedFromNewest() {
    agentChatPage.chatScrollState().should("have.attr", "data-pinned", "false");
  },

  /** Assert the transcript genuinely overflows its viewport, so "scrolled to the newest entry" is a
   *  claim about a scrolled container rather than one short enough to need no scrolling. Compared
   *  loosely because the exact heights depend on rendered row heights, which the spec does not fix. */
  expectTranscriptScrollable() {
    agentChatPage.chatScrollState().should(($mirror) => {
      const scrollHeight = Number($mirror.attr("data-scroll-height"));
      const clientHeight = Number($mirror.attr("data-client-height"));
      expect(scrollHeight, "transcript content height").to.be.greaterThan(clientHeight);
    });
  },

  /** The scroll offset the viewport currently reports — capture it before an act to compare after. */
  readScrollTop: (): Cypress.Chainable<number> =>
    byTestId(TEST_IDS.agentChatScrollState, { timeout: 5000 })
      .invoke("attr", "data-scroll-top")
      .then((value) => Number(value)),

  /** Assert the viewport sits at exactly `offset` — the read position, unmoved. */
  expectScrollTop(offset: number) {
    agentChatPage.readScrollTop().should("equal", offset);
  },

  /**
   * Assert the entry at `index` sits at the top edge of the transcript viewport — the **read
   * position** a reader who scrolled to the top of the loaded range is holding. It is what a
   * prepended older page must not disturb, and the one fact the scroll mirror cannot state: after a
   * prepend the offset legitimately changes, and only the entry staying put proves the change was
   * the compensating one.
   *
   * Compared within a pixel because a scroll offset can land fractionally; the container's own
   * padding is not subtracted, which is sound in the stylesheet-less component harness where it is 0.
   */
  expectEntryAtViewportTop(index: number) {
    agentChatPage.chatMessages().then(($viewport) => {
      const viewportTop = $viewport[0].getBoundingClientRect().top;
      agentChatPage.chatMessage(index).should(($entry) => {
        const entryTop = $entry[0].getBoundingClientRect().top;
        expect(Math.abs(entryTop - viewportTop), `entry ${index} offset from the viewport top`).to.be
          .at.most(1);
      });
    });
  },

  /** The jump-to-latest affordance, shown only while detached; its text carries the arrived count. */
  chatJumpToLatest: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentChatJumpToLatest, { timeout: 5000, ...options }),

  /** Click jump-to-latest, returning to the newest entry and re-attaching. */
  jumpToLatest() {
    byTestId(TEST_IDS.agentChatJumpToLatest).click();
  },

  /** The top-edge indicator shown while an older page is in flight. */
  chatOlderLoading: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentChatOlderLoading, { timeout: 5000, ...options }),

  /** Scroll the transcript to the top of its loaded range — the gesture that pages backwards. */
  scrollTranscriptToTop() {
    agentChatPage.chatMessages().scrollTo("top");
  },

  /** Scroll the transcript back to its newest entry. */
  scrollTranscriptToBottom() {
    agentChatPage.chatMessages().scrollTo("bottom");
  },

  /** The chat text input. */
  chatInput: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentChatInput, { timeout: 5000, ...options }),

  /** The chat send button. */
  chatSendBtn: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentChatSendBtn, { timeout: 5000, ...options }),

  /** The transcript-export button. */
  chatExportBtn: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentChatExportBtn, { timeout: 5000, ...options }),

  /** The chat's inline error banner. */
  chatError: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentChatError, { timeout: 5000, ...options }),

  /** Type a message into the chat input and click Send. */
  sendChatMessage(text: string) {
    byTestId(TEST_IDS.agentChatInput).clear().type(text);
    byTestId(TEST_IDS.agentChatSendBtn).click();
  },

  // ---------------------------------------------------------------------------
  // Clarification question elicitation (AppMode::Select / MultiSelect)
  // ---------------------------------------------------------------------------

  /** The clarification-question panel root, shown while the workflow awaits an answer. */
  chatQuestion: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentChatQuestion, { timeout: 5000, ...options }),

  /** The question's header (short category label, e.g. "Backend"). */
  chatQuestionHeader: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentChatQuestionHeader, { timeout: 5000, ...options }),

  /** The question's full text. */
  chatQuestionText: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentChatQuestionText, { timeout: 5000, ...options }),

  /** A single-select option button, in option order (0-indexed). Clicking answers immediately. */
  chatOption: (index: number, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(agentChatOption(index), { timeout: 5000, ...options }),

  /** A multi-select option checkbox, in option order (0-indexed). Toggling does not answer immediately. */
  chatMultiSelectOption: (index: number, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(agentChatMultiSelectOption(index), { timeout: 5000, ...options }),

  /** Toggle a multi-select checkbox on. */
  toggleMultiSelectOption(index: number) {
    byTestId(agentChatMultiSelectOption(index)).click();
  },

  /** Submit the checked multi-select options (and optional "Other" text). */
  submitMultiSelect() {
    byTestId(TEST_IDS.agentChatMultiSelectSubmit).click();
  },

  /** The free-text "Other" input for a question that allows a custom answer. */
  chatQuestionOtherInput: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentChatQuestionOtherInput, { timeout: 5000, ...options }),

  /** Type into the "Other" input without submitting. */
  typeOtherText(text: string) {
    byTestId(TEST_IDS.agentChatQuestionOtherInput).clear().type(text);
  },

  /** Type and submit a custom "Other" answer for a single-select question. */
  answerOther(text: string) {
    agentChatPage.typeOtherText(text);
    byTestId(TEST_IDS.agentChatQuestionOtherSubmit).click();
  },
};
