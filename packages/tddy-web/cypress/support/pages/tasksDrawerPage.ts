/**
 * Page object for the TasksDrawerScreen acceptance tests.
 *
 * All raw selectors live here; test bodies call named methods.
 */

import {
  byTestId,
  tasksChannelOutput,
  tasksChannelTab,
  tasksDrawerItem,
  tasksDrawerItemCancel,
  tasksDrawerItemKind,
  tasksDrawerItemStatus,
  tasksOutputPaneCancel,
  tasksOutputPaneStatus,
  TEST_IDS,
} from "../testIds";

export const tasksDrawerPage = {
  // ---------------------------------------------------------------------------
  // Screen root
  // ---------------------------------------------------------------------------

  screen: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.tasksDrawerScreen, { timeout: 5000, ...options }),

  // ---------------------------------------------------------------------------
  // Drawer (left sidebar)
  // ---------------------------------------------------------------------------

  drawer: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.tasksDrawer, { timeout: 5000, ...options }),

  drawerItem: (taskId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(tasksDrawerItem(taskId), { timeout: 5000, ...options }),

  drawerItemStatus: (taskId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(tasksDrawerItemStatus(taskId), options),

  drawerItemKind: (taskId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(tasksDrawerItemKind(taskId), options),

  drawerItemCancel: (taskId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(tasksDrawerItemCancel(taskId), { timeout: 5000, ...options }),

  // ---------------------------------------------------------------------------
  // Output pane (right area)
  // ---------------------------------------------------------------------------

  outputPane: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.tasksOutputPane, { timeout: 5000, ...options }),

  outputPaneEmpty: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.tasksOutputPaneEmpty, { timeout: 5000, ...options }),

  outputPaneStatus: (taskId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(tasksOutputPaneStatus(taskId), { timeout: 5000, ...options }),

  outputPaneCancel: (taskId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(tasksOutputPaneCancel(taskId), { timeout: 5000, ...options }),

  // ---------------------------------------------------------------------------
  // Channel tabs and output
  // ---------------------------------------------------------------------------

  channelTab: (taskId: string, channelId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(tasksChannelTab(taskId, channelId), { timeout: 5000, ...options }),

  channelOutput: (taskId: string, channelId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(tasksChannelOutput(taskId, channelId), { timeout: 5000, ...options }),
};
