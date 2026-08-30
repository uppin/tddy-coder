/**
 * Page object for the SessionsDrawerScreen acceptance tests.
 *
 * All raw selectors live here; test bodies call named methods.
 * No raw `cy.get(...)` in test files — only these named helpers.
 */

import {
  byTestId,
  sessionsDrawerItem,
  sessionsDrawerItemLabel,
  sessionsDrawerItemStatus,
  sessionsDrawerItemTooltip,
  sessionsDrawerItemHost,
  sessionsDrawerItemCodebaseHost,
  sessionsDetailResumeBtn,
  sessionsDetailDeleteBtn,
  sessionsInspectorResumeBtn,
  sessionsInspectorDeleteBtn,
  sessionsInspectorDeleteConfirm,
  sessionsInspectorTerminateBtn,
  sessionsVncTargetRow,
  sessionsVncStartBtn,
  sessionsVncStopBtn,
  sessionsVncRemoveBtn,
  sessionsScreenSharingTargetRow,
  sessionsScreenSharingStartBtn,
  sessionsScreenSharingStopBtn,
  sessionsScreenSharingRemoveBtn,
  sessionsRuntimeTerminal,
  sessionsUsageRow,
  sessionsUsageRowAgent,
  sessionsUsageRowModel,
  sessionsUsageRowInput,
  sessionsUsageRowOutput,
  sessionsUsageRowTotal,
  sessionsUsageRowTurns,
  TEST_IDS,
} from "../testIds";

// ---------------------------------------------------------------------------
// Sessions drawer screen — page object
// ---------------------------------------------------------------------------

export const sessionsDrawerPage = {
  // ---------------------------------------------------------------------------
  // Screen root
  // ---------------------------------------------------------------------------

  /** The sessions drawer screen root. */
  screen: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsDrawerScreen, { timeout: 5000, ...options }),

  // ---------------------------------------------------------------------------
  // Drawer (left sidebar)
  // ---------------------------------------------------------------------------

  /** The scrollable drawer containing all session items. */
  drawer: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsDrawer, { timeout: 5000, ...options }),

  /** The close button in the drawer header (collapses to strip on desktop, hides on mobile). */
  drawerCloseBtn: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsDrawerCloseBtn, { timeout: 5000, ...options }),

  /** The open button in strip mode (expands the drawer). */
  drawerOpenBtn: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsDrawerOpenBtn, { timeout: 5000, ...options }),

  /** The floating overlay open button shown on mobile when the list is collapsed. */
  drawerOpenOverlayBtn: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsDrawerOpenOverlayBtn, { timeout: 5000, ...options }),

  /** A single clickable drawer item for the given session id. */
  drawerItem: (sessionId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsDrawerItem(sessionId), { timeout: 5000, ...options }),

  /**
   * Assert the drawer has this session selected. `aria-selected` is set on the drawer item itself,
   * so this states "the app navigated here" without reaching into the detail pane's own chrome.
   */
  expectSessionSelected(sessionId: string) {
    byTestId(sessionsDrawerItem(sessionId), { timeout: 5000 }).should(
      "have.attr",
      "aria-selected",
      "true",
    );
  },

  /** The "Active (N)" partition header (present only when the list has both active and inactive rows). */
  separatorActive: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsDrawerSeparatorActive, { timeout: 5000, ...options }),

  /** The "Remaining (M)" partition header (present only when the list has both active and inactive rows). */
  separatorRemaining: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsDrawerSeparatorRemaining, { timeout: 5000, ...options }),

  /** Expand the (default-collapsed) Remaining partition so its disconnected rows become interactable. */
  expandRemaining: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsDrawerSeparatorRemaining, { timeout: 5000, ...options }).click(),

  /** Asserts the given session's drawer row is the currently selected one. */
  expectSelected: (sessionId: string) => {
    byTestId(sessionsDrawerItem(sessionId), { timeout: 5000 }).should(
      "have.attr",
      "aria-selected",
      "true",
    );
  },

  /** Asserts the given session's drawer row is NOT the currently selected one. */
  expectNotSelected: (sessionId: string) => {
    byTestId(sessionsDrawerItem(sessionId), { timeout: 5000 }).should(
      "not.have.attr",
      "aria-selected",
    );
  },

  /** The derived label text inside a drawer item. */
  drawerItemLabel: (sessionId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsDrawerItemLabel(sessionId), options),

  /** The status indicator dot inside a drawer item. */
  drawerItemStatus: (sessionId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsDrawerItemStatus(sessionId), options),

  /**
   * The row's indicator is in `indicator` — one of `disconnected`, `connected`, `working`,
   * `needs-input` (see `src/lib/sessionIndicator.ts`). The dot carries the token in `data-status`,
   * so a spec states the operator-visible meaning rather than a Tailwind colour class.
   */
  expectIndicator: (sessionId: string, indicator: string, options?: Parameters<typeof cy.get>[1]) => {
    byTestId(sessionsDrawerItemStatus(sessionId), { timeout: 5000, ...options }).should(
      "have.attr",
      "data-status",
      indicator,
    );
  },

  /**
   * The row's dot is fading in and out — the agent is working. The animation is applied as a class
   * (`tddy-session-dot--working`) rather than inspected as a computed style, because a headless
   * runner reports whatever frame it samples and an assertion on opacity would be a coin toss.
   */
  expectIndicatorBlinking: (sessionId: string) => {
    byTestId(sessionsDrawerItemStatus(sessionId), { timeout: 5000 }).should(
      "have.class",
      "tddy-session-dot--working",
    );
  },

  /** The row's dot is steady — whatever colour it is, it is not blinking. */
  expectIndicatorSteady: (sessionId: string) => {
    byTestId(sessionsDrawerItemStatus(sessionId), { timeout: 5000 }).should(
      "not.have.class",
      "tddy-session-dot--working",
    );
  },

  /** The tooltip content element (visible on hover) that contains the full session id. */
  drawerItemTooltip: (sessionId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsDrawerItemTooltip(sessionId), options),

  /** The owning-host badge inside a drawer item (only present on cross-host rows). */
  drawerItemHost: (sessionId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsDrawerItemHost(sessionId), { timeout: 5000, ...options }),

  /** The row carries no owning-host badge — its agent runs on the selected host. */
  expectNoOwningHostBadge(sessionId: string) {
    byTestId(sessionsDrawerItemHost(sessionId)).should("not.exist");
  },

  /**
   * The codebase-host badge — present only on a split session, whose worktree lives on a different
   * daemon than its agent. See docs/ft/daemon/remote-managed-worktree.md.
   */
  codebaseHostBadge: (sessionId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsDrawerItemCodebaseHost(sessionId), { timeout: 5000, ...options }),

  /** The row's codebase is on the same daemon as its agent, so no second badge is rendered. */
  expectNoCodebaseHostBadge(sessionId: string) {
    byTestId(sessionsDrawerItemCodebaseHost(sessionId)).should("not.exist");
  },

  // ---------------------------------------------------------------------------
  // Detail pane (right area)
  // ---------------------------------------------------------------------------

  /** The detail pane container (right of the drawer). */
  detailPane: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsDetailPane, { timeout: 5000, ...options }),

  /** The terminal container rendered when a connected session is selected. */
  detailTerminalContainer: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsDetailTerminalContainer, { timeout: 10000, ...options }),

  /** The "Code" header toggle that opens the worktree Code pane for the selected session. */
  codeToggle: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.worktreeCodeToggle, { timeout: 5000, ...options }),

  /** The metadata block rendered when a disconnected session is selected. */
  detailMetadata: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsDetailMetadata, { timeout: 5000, ...options }),

  /** The Resume button rendered for a disconnected session. */
  detailResumeBtn: (sessionId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsDetailResumeBtn(sessionId), { timeout: 5000, ...options }),

  /** The Delete button rendered for a disconnected session in the detail pane. */
  detailDeleteBtn: (sessionId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsDetailDeleteBtn(sessionId), { timeout: 5000, ...options }),

  // ---------------------------------------------------------------------------
  // Inspector drawer (right overlay)
  // ---------------------------------------------------------------------------

  /** The inspector drawer element — check data-state attribute ("closed"|"open"|"expanded"). */
  inspectorDrawer: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsInspectorDrawer, { timeout: 5000, ...options }),

  /** The toggle button that opens/closes the inspector. */
  inspectorToggle: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsInspectorToggle, { timeout: 5000, ...options }),

  /** The close button inside the inspector header. */
  inspectorClose: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsInspectorClose, { timeout: 5000, ...options }),

  /** The expand button inside the inspector header. */
  inspectorExpand: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsInspectorExpand, { timeout: 5000, ...options }),

  /** The restore button inside the inspector header (visible only in expanded state). */
  inspectorRestore: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsInspectorRestore, { timeout: 5000, ...options }),

  /** The metadata section inside the inspector. */
  inspectorMetadata: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsInspectorMetadata, { timeout: 5000, ...options }),


  /** The Resume button inside the inspector for the given session. */
  inspectorResumeBtn: (sessionId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsInspectorResumeBtn(sessionId), { timeout: 5000, ...options }),

  /** The Delete button inside the inspector for the given session. */
  inspectorDeleteBtn: (sessionId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsInspectorDeleteBtn(sessionId), { timeout: 5000, ...options }),

  /** The confirm-delete button (second click) inside the inspector. */
  inspectorDeleteConfirm: (sessionId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsInspectorDeleteConfirm(sessionId), { timeout: 5000, ...options }),

  /** The Terminate button inside the inspector for the given session. */
  inspectorTerminateBtn: (sessionId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsInspectorTerminateBtn(sessionId), { timeout: 5000, ...options }),

  // ---------------------------------------------------------------------------
  // Inspector tab strip
  // ---------------------------------------------------------------------------

  /** The Details tab button in the inspector tab strip. */
  inspectorDetailsTab: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsInspectorTabDetails, { timeout: 5000, ...options }),

  /** The Tools tab button in the inspector tab strip. */
  inspectorToolsTab: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsInspectorTabTools, { timeout: 5000, ...options }),

  /** The Agents tab button in the inspector tab strip — reveals the session's agent roster. */
  inspectorAgentsTab: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsInspectorTabAgents, { timeout: 5000, ...options }),

  /** The Worktree tab button in the inspector tab strip. */
  inspectorWorktreeTab: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionInspectorTabWorktree, { timeout: 5000, ...options }),

  /** The Files tab button in the inspector tab strip. */
  inspectorFilesTab: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionInspectorTabFiles, { timeout: 5000, ...options }),

  /** Asserts the inspector drawer is in the given visual state. */
  expectInspectorState: (state: "closed" | "open" | "expanded") => {
    byTestId(TEST_IDS.sessionsInspectorDrawer, { timeout: 5000 }).should(
      "have.attr",
      "data-state",
      state,
    );
  },

  // ---------------------------------------------------------------------------
  // Usage tab
  // ---------------------------------------------------------------------------

  /** The Usage tab button in the inspector tab strip. */
  inspectorUsageTab: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsInspectorTabUsage, { timeout: 5000, ...options }),

  /** The Usage tab panel (rendered when the Usage tab is active). */
  usageTabPanel: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsUsageTabPanel, { timeout: 5000, ...options }),

  /** The zero/empty state shown before any usage snapshot arrives. */
  usageEmpty: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsUsageEmpty, { timeout: 5000, ...options }),

  /** A single conversation row, keyed by conversation id. */
  usageRow: (id: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsUsageRow(id), { timeout: 5000, ...options }),

  /** The agent cell of a conversation row. */
  usageRowAgent: (id: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsUsageRowAgent(id), { timeout: 5000, ...options }),

  /** The model cell of a conversation row. */
  usageRowModel: (id: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsUsageRowModel(id), { timeout: 5000, ...options }),

  /** The input-tokens cell of a conversation row. */
  usageRowInput: (id: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsUsageRowInput(id), { timeout: 5000, ...options }),

  /** The output-tokens cell of a conversation row. */
  usageRowOutput: (id: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsUsageRowOutput(id), { timeout: 5000, ...options }),

  /** The total-tokens cell of a conversation row. */
  usageRowTotal: (id: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsUsageRowTotal(id), { timeout: 5000, ...options }),

  /** The turns cell of a conversation row. */
  usageRowTurns: (id: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsUsageRowTurns(id), { timeout: 5000, ...options }),

  /** The input-tokens cell of the TOTAL row. */
  usageTotalInput: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsUsageTotalInput, { timeout: 5000, ...options }),

  /** The output-tokens cell of the TOTAL row. */
  usageTotalOutput: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsUsageTotalOutput, { timeout: 5000, ...options }),

  /** The total-tokens cell of the TOTAL row. */
  usageTotalTotal: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsUsageTotalTotal, { timeout: 5000, ...options }),

  // ---------------------------------------------------------------------------
  // VNC tab
  // ---------------------------------------------------------------------------

  /** The VNC tab button in the inspector tab strip. */
  inspectorVncTab: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsInspectorTabVnc, { timeout: 5000, ...options }),

  /** The VNC tab panel (rendered when the VNC tab is active). */
  vncTabPanel: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsVncTabPanel, { timeout: 5000, ...options }),

  /** The list of VNC targets. */
  vncTargetList: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsVncTargetList, { timeout: 5000, ...options }),

  /** A single VNC target row. */
  vncTargetRow: (targetId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsVncTargetRow(targetId), { timeout: 5000, ...options }),

  /** The Start button for a given target. */
  vncStartBtn: (targetId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsVncStartBtn(targetId), { timeout: 5000, ...options }),

  /** The Stop button for a given target. */
  vncStopBtn: (targetId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsVncStopBtn(targetId), { timeout: 5000, ...options }),

  /** The Remove button for a given target. */
  vncRemoveBtn: (targetId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsVncRemoveBtn(targetId), { timeout: 5000, ...options }),

  /** The Add VNC target form. */
  vncAddForm: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsVncAddForm, { timeout: 5000, ...options }),

  /** The label input in the Add form. */
  vncAddLabel: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsVncAddLabel, options),

  /** The host input in the Add form. */
  vncAddHost: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsVncAddHost, options),

  /** The port input in the Add form. */
  vncAddPort: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsVncAddPort, options),

  /** The password input in the Add form. */
  vncAddPassword: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsVncAddPassword, options),

  /** The submit button in the Add form. */
  vncAddSubmit: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsVncAddSubmit, { timeout: 5000, ...options }),

  /** The passphrase dialog. */
  vncPassphraseDialog: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsVncPassphraseDialog, { timeout: 5000, ...options }),

  /** The passphrase input in the dialog. */
  vncPassphraseInput: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsVncPassphraseInput, options),

  /** The confirm button in the passphrase dialog. */
  vncPassphraseConfirm: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsVncPassphraseConfirm, { timeout: 5000, ...options }),

  // ---------------------------------------------------------------------------
  // VNC overlay
  // ---------------------------------------------------------------------------

  /** The full-screen VNC desktop overlay. */
  vncOverlay: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.vncOverlay, { timeout: 5000, ...options }),

  /** The close button inside the VNC overlay. */
  vncOverlayClose: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.vncOverlayClose, { timeout: 5000, ...options }),

  /** The `<video>` element inside the VNC overlay. */
  vncOverlayVideo: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.vncOverlayVideo, { timeout: 5000, ...options }),

  // ---------------------------------------------------------------------------
  // Terminal control mutex — "Claim terminal" CTA
  // ---------------------------------------------------------------------------

  /** The overlay that appears when this screen is not the terminal controller. */
  terminalControlOverlay: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.terminalControlOverlay, { timeout: 5000, ...options }),

  /** The "Claim terminal" button inside the control overlay. */
  terminalClaimBtn: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.terminalClaimBtn, { timeout: 5000, ...options }),

  /** The text element naming the screen currently holding control. */
  terminalControlHolder: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.terminalControlHolder, { timeout: 5000, ...options }),

  // ---------------------------------------------------------------------------
  // Screen Sharing tab
  // ---------------------------------------------------------------------------

  /** The Screen Sharing tab button in the inspector tab strip. */
  inspectorScreenSharingTab: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsInspectorTabScreenSharing, { timeout: 5000, ...options }),

  /** The Screen Sharing tab panel (rendered when the Screen Sharing tab is active). */
  screenSharingTabPanel: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsScreenSharingTabPanel, { timeout: 5000, ...options }),

  /** The list of screen-sharing targets. */
  screenSharingTargetList: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsScreenSharingTargetList, { timeout: 5000, ...options }),

  /** A single screen-sharing target row. */
  screenSharingTargetRow: (targetId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsScreenSharingTargetRow(targetId), { timeout: 5000, ...options }),

  /** The Start button for a given screen-sharing target. */
  screenSharingStartBtn: (targetId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsScreenSharingStartBtn(targetId), { timeout: 5000, ...options }),

  /** The Stop button for a given screen-sharing target. */
  screenSharingStopBtn: (targetId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsScreenSharingStopBtn(targetId), { timeout: 5000, ...options }),

  /** The Remove button for a given screen-sharing target. */
  screenSharingRemoveBtn: (targetId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsScreenSharingRemoveBtn(targetId), { timeout: 5000, ...options }),

  /** The Add screen-sharing target form. */
  screenSharingAddForm: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsScreenSharingAddForm, { timeout: 5000, ...options }),

  /** The label input in the Add form. */
  screenSharingAddLabel: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsScreenSharingAddLabel, options),

  /** The host input in the Add form. */
  screenSharingAddHost: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsScreenSharingAddHost, options),

  /** The port input in the Add form. */
  screenSharingAddPort: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsScreenSharingAddPort, options),

  /** The password input in the Add form. */
  screenSharingAddPassword: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsScreenSharingAddPassword, options),

  /** The protocol selector in the Add form (VNC | RDP). */
  screenSharingAddProtocol: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsScreenSharingAddProtocol, options),

  /** The submit button in the Add form. */
  screenSharingAddSubmit: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsScreenSharingAddSubmit, { timeout: 5000, ...options }),

  /** The passphrase dialog. */
  screenSharingPassphraseDialog: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsScreenSharingPassphraseDialog, { timeout: 5000, ...options }),

  /** The passphrase input in the dialog. */
  screenSharingPassphraseInput: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsScreenSharingPassphraseInput, options),

  /** The confirm button in the passphrase dialog. */
  screenSharingPassphraseConfirm: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsScreenSharingPassphraseConfirm, { timeout: 5000, ...options }),

  // ---------------------------------------------------------------------------
  // Screen Sharing overlay
  // ---------------------------------------------------------------------------

  /** The full-screen screen-sharing desktop overlay. */
  screenSharingOverlay: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.screenSharingOverlay, { timeout: 5000, ...options }),

  /** The close button inside the screen-sharing overlay. */
  screenSharingOverlayClose: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.screenSharingOverlayClose, { timeout: 5000, ...options }),

  /** The `<video>` element inside the screen-sharing overlay. */
  screenSharingOverlayVideo: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.screenSharingOverlayVideo, { timeout: 5000, ...options }),

  // ---------------------------------------------------------------------------
  // Fast Session Change — runtime registry + background terminals + inspector I/O
  // ---------------------------------------------------------------------------

  /** The hidden layer that keeps one mounted terminal per attached session. */
  runtimeLayer: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsRuntimeLayer, { timeout: 5000, ...options }),

  /** A per-session terminal container mounted in the runtime registry. */
  runtimeTerminal: (sessionId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsRuntimeTerminal(sessionId), { timeout: 10000, ...options }),

  /** The focusable input of a session's runtime terminal (the xterm helper textarea) — the element
   *  that receives DOM focus when the terminal is focused, so a test can assert keyboard readiness
   *  without a click. */
  runtimeTerminalInput: (sessionId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsRuntimeTerminal(sessionId), { timeout: 10000, ...options }).find("textarea"),

  /** Cumulative inbound bytes for the focused session (inspector Details tab). */
  inspectorBytesIn: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsInspectorBytesIn, { timeout: 5000, ...options }),

  /** Cumulative outbound bytes for the focused session (inspector Details tab). */
  inspectorBytesOut: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsInspectorBytesOut, { timeout: 5000, ...options }),

  /** The "last data received: Ns ago" relative timestamp (inspector Details tab). */
  inspectorLastDataReceived: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsInspectorLastDataReceived, { timeout: 5000, ...options }),

  /** The parsed `session` participant-metadata block on a drawer row. */
  drawerItemSessionMeta: (sessionId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(
      `${TEST_IDS.sessionsDrawerItemSessionMeta}-${sessionId}`,
      { timeout: 5000, ...options },
    ),

  // ---------------------------------------------------------------------------
  // Create-session pane (shared by the drawer's "new session" flow and the
  // session-detail "Add agent" peer-spawn flow)
  // ---------------------------------------------------------------------------

  /** The "+ New session" button in the drawer header, which opens the creation pane. */
  newSessionBtn: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsDrawerNewBtn, { timeout: 5000, ...options }),

  /** The shared session-creation pane (rendered inline or in a dialog). */
  createSessionPane: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.createSessionPane, { timeout: 5000, ...options }),

  /** The shared session-creation dialog wrapper (when rendered as a modal). */
  createSessionDialog: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.createSessionDialog, { timeout: 5000, ...options }),

  /** The submit button on the shared session-creation pane. */
  createSessionSubmitBtn: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.createSessionSubmitBtn, { timeout: 5000, ...options }),
};
