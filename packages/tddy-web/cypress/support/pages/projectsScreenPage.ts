/**
 * Page object for the Projects screen (`/projects`) acceptance tests.
 *
 * All raw selectors live here; test bodies call named methods.
 * No raw `cy.get(...)` in test files — only these named helpers.
 */

import {
  byTestId,
  projectCard,
  projectHostRow,
  projectAddToHostToggle,
  projectAddToHostSelect,
  projectAddToHostSubmit,
  projectAddToHostUserRelativePath,
  projectHostBaseLocation,
  TEST_IDS,
} from "../testIds";

export const projectsScreenPage = {
  // ---------------------------------------------------------------------------
  // Screen root + project cards
  // ---------------------------------------------------------------------------

  /** The Projects screen root. */
  screen: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.projectsScreen, { timeout: 5000, ...options }),

  /** The list container for project cards. */
  list: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.projectsList, { timeout: 5000, ...options }),

  /** A single project card for the given logical project id. */
  card: (projectId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(projectCard(projectId), { timeout: 5000, ...options }),

  /** One host row (per hosting daemon) inside a project card. */
  hostRow: (
    projectId: string,
    daemonInstanceId: string,
    options?: Parameters<typeof cy.get>[1],
  ) => byTestId(projectHostRow(projectId, daemonInstanceId), { timeout: 5000, ...options }),

  /** The daemon-instance ids of every host row rendered for a project, in DOM order. */
  hostRowDaemonIds: (projectId: string): Cypress.Chainable<string[]> =>
    projectsScreenPage
      .card(projectId)
      .find(`[data-testid^='project-host-row-${projectId}-']`)
      .then(($rows) =>
        [...$rows].map((el) =>
          el.getAttribute("data-testid")!.replace(`project-host-row-${projectId}-`, ""),
        ),
      ),

  // ---------------------------------------------------------------------------
  // Add to host
  // ---------------------------------------------------------------------------

  /** Opens the add-to-host control for a project. */
  openAddToHost(projectId: string) {
    byTestId(projectAddToHostToggle(projectId)).click();
  },

  /** The target-host `<select>` for a project. */
  addToHostSelect: (projectId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(projectAddToHostSelect(projectId), { timeout: 5000, ...options }),

  /** The daemon-instance ids offered as add-to-host targets, in option order. */
  addToHostOptionValues: (projectId: string): Cypress.Chainable<string[]> =>
    projectsScreenPage
      .addToHostSelect(projectId)
      .find("option")
      .then(($opts) => [...$opts].map((el) => (el as HTMLOptionElement).value)),

  /** Select a target host and submit the add-to-host action for a project. */
  addProjectToHost(projectId: string, daemonInstanceId: string) {
    byTestId(projectAddToHostSelect(projectId)).select(daemonInstanceId);
    byTestId(projectAddToHostSubmit(projectId)).click();
  },

  /** The optional clone-location input inside the add-to-host control. */
  addToHostUserRelativePathInput: (
    projectId: string,
    options?: Parameters<typeof cy.get>[1],
  ) => byTestId(projectAddToHostUserRelativePath(projectId), { timeout: 5000, ...options }),

  /** Select a target host, type a clone-location relative path, and submit the add-to-host action. */
  addProjectToHostWithLocation(projectId: string, daemonInstanceId: string, relativePath: string) {
    byTestId(projectAddToHostSelect(projectId)).select(daemonInstanceId);
    byTestId(projectAddToHostUserRelativePath(projectId)).clear().type(relativePath);
    byTestId(projectAddToHostSubmit(projectId)).click();
  },

  /** A host's advertised base clone location (`repos_base_path`) rendered in the Projects screen. */
  hostBaseLocation: (daemonInstanceId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(projectHostBaseLocation(daemonInstanceId), { timeout: 5000, ...options }),

  // ---------------------------------------------------------------------------
  // Create project
  // ---------------------------------------------------------------------------

  /** Opens the create-project form. */
  openCreateProjectForm() {
    byTestId(TEST_IDS.projectsCreateProjectToggle).click();
  },

  /** Fill and submit the create-project form (name + git URL; optional relative path). */
  fillAndSubmitCreateProjectForm(options: {
    name: string;
    gitUrl: string;
    userRelativePath?: string;
  }) {
    byTestId(TEST_IDS.projectsNewProjectName).clear().type(options.name);
    byTestId(TEST_IDS.projectsNewProjectGitUrl).clear().type(options.gitUrl);
    if (options.userRelativePath) {
      byTestId(TEST_IDS.projectsNewProjectUserRelativePath)
        .clear()
        .type(options.userRelativePath);
    }
    byTestId(TEST_IDS.projectsCreateProjectSubmit).click();
  },
};
