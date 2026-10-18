/**
 * Page object for the Worktree Code pane (docs/ft/web/session-code-pane.md).
 *
 * All raw selectors live here; test bodies call named methods — no raw `cy.get(...)` in specs.
 */

import {
  byTestId,
  TEST_IDS,
  worktreeCodeIdentifier,
  worktreeCodeLine,
  worktreeCodeReference,
  worktreeTreeNode,
} from "../testIds";

export const worktreeCodePanePage = {
  /** The Code toggle button in the main-pane header (present for every session type). */
  toggle: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.worktreeCodeToggle, { timeout: 5000, ...options }),

  /** The split Code pane container (present only when the pane is open). */
  pane: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.worktreeCodePane, { timeout: 5000, ...options }),

  /** The bottom strip showing the session's code-index status; `data-state` names the state. */
  indexStatus: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.worktreeCodeIndexStatus, { timeout: 5000, ...options }),

  /** The index strip's headline: phase and percentage, ready, or failed. */
  indexStatusSummary: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.worktreeCodeIndexStatusSummary, { timeout: 5000, ...options }),

  /** The index strip's detail: the message of the phase the headline names. */
  indexStatusDetail: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.worktreeCodeIndexStatusDetail, { timeout: 5000, ...options }),

  /** The strip's `+N more` while several index phases run; its `title` lists every phase. */
  indexStatusMore: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.worktreeCodeIndexStatusMore, { timeout: 5000, ...options }),

  /** The directory tree region inside the Code pane. */
  tree: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.worktreeFileTree, { timeout: 5000, ...options }),

  /** A single tree node (file or directory) keyed by its path relative to the worktree root. */
  node: (relPath: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(worktreeTreeNode(relPath), { timeout: 5000, ...options }),

  /** The read-only file preview region inside the Code pane. */
  preview: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.worktreeFilePreview, { timeout: 5000, ...options }),

  /** The syntax-highlighted code block inside the preview (present only for recognized languages). */
  highlight: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.worktreeCodeHighlight, { timeout: 5000, ...options }),

  /** One one-based line of the code preview. */
  line: (line: number, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(worktreeCodeLine(line), { timeout: 5000, ...options }),

  /** The identifier token starting at a one-based line and one-based byte column. */
  identifier: (line: number, column: number, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(worktreeCodeIdentifier(line, column), { timeout: 5000, ...options }),

  /** The hover card showing an identifier's type. */
  hoverCard: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.worktreeCodeHover, { timeout: 5000, ...options }),

  /** The hover card's scrolling body — the rendered hover markdown. */
  hoverBody: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.worktreeCodeHoverBody, { timeout: 5000, ...options }),

  /** Heading elements (`h1`–`h6`) rendered inside the hover card's body. */
  hoverBodyHeadings: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.worktreeCodeHoverBody, { timeout: 5000, ...options }).find(
      "h1, h2, h3, h4, h5, h6",
    ),

  /** Paragraph elements rendered inside the hover card's body. */
  hoverBodyParagraphs: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.worktreeCodeHoverBody, { timeout: 5000, ...options }).find("p"),

  /** The hover card's loading state while its query is held by a loading index. */
  hoverLoading: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.worktreeCodeHoverLoading, { timeout: 5000, ...options }),

  /** The hover card's Close action. */
  hoverClose: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.worktreeCodeHoverClose, { timeout: 5000, ...options }),

  /** The navigation notice (go-to-definition outcome, held state, failure). */
  notice: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.worktreeCodeNotice, { timeout: 5000, ...options }),

  /** The action that lists the references of the hovered identifier. */
  referencesAction: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.worktreeCodeReferencesAction, { timeout: 5000, ...options }),

  /** The references list. */
  references: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.worktreeCodeReferences, { timeout: 5000, ...options }),

  /** One entry of the references list, keyed by the file and one-based line it points at. */
  reference: (relPath: string, line: number, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(worktreeCodeReference(relPath, line), { timeout: 5000, ...options }),
};
