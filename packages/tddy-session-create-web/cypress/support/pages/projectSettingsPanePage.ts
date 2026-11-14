/**
 * Page object for a project's settings pane mounted on its own (`#orchestrators` 11/11): main's
 * section, the orchestrator defaults section and its controls, and the
 * `SetProjectOrchestratorDefaults` calls the daemon double recorded.
 *
 * All raw selectors and wire inspection live here; test bodies call named methods.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-10-project-settings-pane.md
 */

import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { ProjectService } from "tddy-client-web/gen/project_pb";
import {
  TEST_IDS,
  byTestId,
  sessionsProjectHomeSetup,
  sessionsProjectHomeSetupReplacing,
  sessionsProjectHomeSetupUnknown,
  sessionsProjectSettings,
  sessionsProjectSettingsDefaults,
  sessionsProjectSettingsDefaultsError,
  sessionsProjectSettingsDefaultsSave,
  sessionsProjectSettingsMain,
  sessionsProjectSettingsReplaceMain,
} from "../testIds";

/** How long a lookup waits for something the pane renders after an RPC answers. */
const RENDER_WAIT = { timeout: 5000 };

/** A daemon double recording the unary calls it answered. */
type RecordingBackend = Pick<InMemoryRpcBackend, "callsTo">;

/** The value each defaults control takes for "the agent's own default". */
export const AGENT_DEFAULT = { model: "", effort: "", provider: "builtin" } as const;

const within = (section: () => Cypress.Chainable<JQuery<HTMLElement>>, id: string) =>
  section().find(`[data-testid='${id}']`, RENDER_WAIT);

export const projectSettingsPanePage = {
  pane: (projectId: string) => byTestId(sessionsProjectSettings(projectId), RENDER_WAIT),

  // ---------------------------------------------------------------------------
  // Main
  // ---------------------------------------------------------------------------

  mainSection: (projectId: string) => byTestId(sessionsProjectSettingsMain(projectId), RENDER_WAIT),

  mainForm: (projectId: string) =>
    within(() => projectSettingsPanePage.mainSection(projectId), sessionsProjectHomeSetup(projectId)),

  replacingNotice: (projectId: string) =>
    within(() => projectSettingsPanePage.mainSection(projectId), sessionsProjectHomeSetupReplacing(projectId)),

  unknownProject: (projectId: string) => byTestId(sessionsProjectHomeSetupUnknown(projectId), RENDER_WAIT),

  replaceMain: (projectId: string) => byTestId(sessionsProjectSettingsReplaceMain(projectId), RENDER_WAIT),

  clickReplaceMain(projectId: string) {
    projectSettingsPanePage.replaceMain(projectId).click();
  },

  /** Asserts the main section offers no Replace main. */
  assertNoReplaceMain(projectId: string) {
    projectSettingsPanePage.mainSection(projectId).should("exist");
    byTestId(sessionsProjectSettingsReplaceMain(projectId)).should("not.exist");
  },

  // ---------------------------------------------------------------------------
  // Orchestrator defaults
  // ---------------------------------------------------------------------------

  defaultsSection: (projectId: string) => byTestId(sessionsProjectSettingsDefaults(projectId), RENDER_WAIT),

  modelSelect: (projectId: string) =>
    within(() => projectSettingsPanePage.defaultsSection(projectId), TEST_IDS.createSessionModelSelect),

  effortSelect: (projectId: string) =>
    within(() => projectSettingsPanePage.defaultsSection(projectId), TEST_IDS.createSessionEffortSelect),

  providerSelect: (projectId: string) =>
    within(() => projectSettingsPanePage.defaultsSection(projectId), TEST_IDS.createSessionProviderSelect),

  chooseDefaults(projectId: string, choice: { model: string; effort: string; provider: string }) {
    projectSettingsPanePage.modelSelect(projectId).select(choice.model);
    projectSettingsPanePage.effortSelect(projectId).select(choice.effort);
    projectSettingsPanePage.providerSelect(projectId).select(choice.provider);
  },

  save(projectId: string) {
    byTestId(sessionsProjectSettingsDefaultsSave(projectId), RENDER_WAIT).should("not.be.disabled").click();
  },

  defaultsError: (projectId: string) => byTestId(sessionsProjectSettingsDefaultsError(projectId), RENDER_WAIT),

  /** The one `SetProjectOrchestratorDefaults` request the pane sent. */
  theSaveRequest: (backend: RecordingBackend) =>
    cy
      .wrap(backend)
      .should((b) => {
        expect(
          b.callsTo(ProjectService.method.setProjectOrchestratorDefaults),
          "one SetProjectOrchestratorDefaults request",
        ).to.have.length(1);
      })
      .then((b) => b.callsTo(ProjectService.method.setProjectOrchestratorDefaults)[0]),
};
