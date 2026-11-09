/**
 * Page object for `SessionMainPane`: its detail pane, header toggles and the inspector's tab strip.
 *
 * All raw selectors live here; test bodies call named methods.
 */

import { byTestId, TEST_IDS } from "../testIds";

type GetOptions = Parameters<typeof cy.get>[1];

export const sessionMainPanePage = {
  /** The detail pane rendered for the selected session. */
  detailPane: (options?: GetOptions) => byTestId(TEST_IDS.sessionsDetailPane, { timeout: 5000, ...options }),

  /** The terminal container rendered when a connected session is selected. */
  detailTerminalContainer: (options?: GetOptions) =>
    byTestId(TEST_IDS.sessionsDetailTerminalContainer, { timeout: 10000, ...options }),

  /** The "Code" header toggle that opens the worktree Code pane for the selected session. */
  codeToggle: (options?: GetOptions) => byTestId(TEST_IDS.worktreeCodeToggle, { timeout: 5000, ...options }),

  /** The control toast over the terminal. */
  terminalControlOverlay: (options?: GetOptions) =>
    byTestId(TEST_IDS.terminalControlOverlay, { timeout: 5000, ...options }),

  /** The metadata block of the inspector. */
  inspectorMetadata: (options?: GetOptions) =>
    byTestId(TEST_IDS.sessionsInspectorMetadata, { timeout: 5000, ...options }),

  inspectorDetailsTab: (options?: GetOptions) =>
    byTestId(TEST_IDS.sessionsInspectorTabDetails, { timeout: 5000, ...options }),
  inspectorToolsTab: (options?: GetOptions) =>
    byTestId(TEST_IDS.sessionsInspectorTabTools, { timeout: 5000, ...options }),
  inspectorAgentsTab: (options?: GetOptions) =>
    byTestId(TEST_IDS.sessionsInspectorTabAgents, { timeout: 5000, ...options }),
  inspectorWorktreeTab: (options?: GetOptions) =>
    byTestId(TEST_IDS.sessionInspectorTabWorktree, { timeout: 5000, ...options }),
  inspectorFilesTab: (options?: GetOptions) =>
    byTestId(TEST_IDS.sessionInspectorTabFiles, { timeout: 5000, ...options }),
  inspectorUsageTab: (options?: GetOptions) =>
    byTestId(TEST_IDS.sessionsInspectorTabUsage, { timeout: 5000, ...options }),
  inspectorVncTab: (options?: GetOptions) =>
    byTestId(TEST_IDS.sessionsInspectorTabVnc, { timeout: 5000, ...options }),
  inspectorScreenSharingTab: (options?: GetOptions) =>
    byTestId(TEST_IDS.sessionsInspectorTabScreenSharing, { timeout: 5000, ...options }),

  /** The VNC tab panel (rendered when the VNC tab is active). */
  vncTabPanel: (options?: GetOptions) => byTestId(TEST_IDS.sessionsVncTabPanel, { timeout: 5000, ...options }),

  /** The Screen Sharing tab panel (rendered when the Screen Sharing tab is active). */
  screenSharingTabPanel: (options?: GetOptions) =>
    byTestId(TEST_IDS.sessionsScreenSharingTabPanel, { timeout: 5000, ...options }),
};
