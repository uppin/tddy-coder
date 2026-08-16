/**
 * Page object for the Models & Agents screen (`#/models`) acceptance tests.
 *
 * All raw selectors live here; test bodies call named methods.
 * No raw `cy.get(...)` in test files — only these named helpers.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md.
 */

import {
  byTestId,
  modelsAssistantRow,
  modelsAssistantTools,
  modelsCreateAssistantTool,
  modelsDaemonError,
  modelsProviderCredential,
  modelsProviderError,
  modelsProviderRefresh,
  modelsProviderRow,
  modelsRow,
  modelsRowChat,
  modelsRowCreateAssistant,
  modelsRowDaemon,
  modelsRowError,
  modelsRowLabels,
  modelsRowLoad,
  modelsRowLoadState,
  modelsRowStale,
  modelsRowUnload,
  TEST_IDS,
} from "../testIds";

/** Identifies one model row across the merged, cross-daemon table. */
export interface ModelRef {
  daemonInstanceId: string;
  providerId: string;
  modelId: string;
}

/**
 * Identifies one provider row across the merged, cross-daemon panel. The daemon is part of the
 * reference because provider ids are minted per daemon — `prov-ollama` names a different provider
 * on every host.
 */
export interface ProviderRef {
  daemonInstanceId: string;
  providerId: string;
}

const rowId = (m: ModelRef) => modelsRow(m.daemonInstanceId, m.providerId, m.modelId);

/**
 * The value of an attribute the screen is expected to publish.
 *
 * A component that stopped emitting the attribute is a regression, so it throws here rather than
 * substituting `""` — which would turn "the row publishes no labels at all" into "the row has an
 * empty label list" and quietly pass a spec that seeds no labels either.
 */
function requiredAttribute(value: string | undefined, attribute: string, what: string): string {
  if (value === undefined) {
    throw new Error(`${what} is missing its '${attribute}' attribute`);
  }
  return value;
}

/** A comma-separated attribute as a list; an empty attribute is an empty list. */
function attributeList(value: string): string[] {
  return value.split(",").filter((entry) => entry.length > 0);
}

export const modelsScreenPage = {
  // ---------------------------------------------------------------------------
  // Screen root
  // ---------------------------------------------------------------------------

  /** The Models & Agents screen root. */
  screen: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.modelsScreen, { timeout: 5000, ...options }),

  /** The models table. */
  table: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.modelsTable, { timeout: 5000, ...options }),

  /**
   * Why the table has no model rows, read from `data-registry-status` — `not-connected`,
   * `no-daemons`, `loading` or `ready`. Absent whenever the table has rows to show.
   */
  emptyStateStatus: (): Cypress.Chainable<string> =>
    byTestId(TEST_IDS.modelsTableEmpty, { timeout: 5000 })
      .invoke("attr", "data-registry-status")
      .then((value) =>
        requiredAttribute(value, "data-registry-status", "the models table's empty state"),
      ),

  /** The row the table shows in place of models. */
  emptyState: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.modelsTableEmpty, { timeout: 5000, ...options }),

  // ---------------------------------------------------------------------------
  // Model rows
  // ---------------------------------------------------------------------------

  /** One model row in the merged table. */
  row: (model: ModelRef, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(rowId(model), { timeout: 5000, ...options }),

  /** The owning-daemon instance id rendered on a model row. */
  rowDaemon: (model: ModelRef): Cypress.Chainable<string> =>
    byTestId(modelsRowDaemon(model.daemonInstanceId, model.providerId, model.modelId), {
      timeout: 5000,
    }).invoke("text"),

  /**
   * The capability labels of a model row, read from `data-model-labels` (a comma-separated
   * list) rather than rendered text, so the assertion is exact and independent of chip styling.
   */
  rowLabels: (model: ModelRef): Cypress.Chainable<string[]> =>
    byTestId(modelsRowLabels(model.daemonInstanceId, model.providerId, model.modelId), {
      timeout: 5000,
    })
      .invoke("attr", "data-model-labels")
      .then((value) =>
        attributeList(
          requiredAttribute(value, "data-model-labels", `model row ${model.modelId}`),
        ),
      ),

  /** The load state of a model row, read from `data-load-state`. */
  rowLoadState: (model: ModelRef): Cypress.Chainable<string> =>
    byTestId(modelsRowLoadState(model.daemonInstanceId, model.providerId, model.modelId), {
      timeout: 5000,
    })
      .invoke("attr", "data-load-state")
      .then((value) =>
        requiredAttribute(value, "data-load-state", `model row ${model.modelId}`),
      ),

  /**
   * Whether a model row is marked stale, read from `data-stale`: `"true"` when the row's provider
   * could not be enumerated, so the row is the last catalog that worked rather than a current one.
   */
  rowIsStale: (model: ModelRef): Cypress.Chainable<string> =>
    modelsScreenPage
      .row(model)
      .invoke("attr", "data-stale")
      .then((value) => requiredAttribute(value, "data-stale", `model row ${model.modelId}`)),

  /** The visible stale marker on a model row. */
  rowStaleMarker: (model: ModelRef, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(modelsRowStale(model.daemonInstanceId, model.providerId, model.modelId), {
      timeout: 5000,
      ...options,
    }),

  /** The Load action on a model row. */
  loadButton: (model: ModelRef, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(modelsRowLoad(model.daemonInstanceId, model.providerId, model.modelId), {
      timeout: 5000,
      ...options,
    }),

  /** The Unload action on a model row. */
  unloadButton: (model: ModelRef, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(modelsRowUnload(model.daemonInstanceId, model.providerId, model.modelId), {
      timeout: 5000,
      ...options,
    }),

  /** The Chat action on a model row. */
  chatButton: (model: ModelRef, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(modelsRowChat(model.daemonInstanceId, model.providerId, model.modelId), {
      timeout: 5000,
      ...options,
    }),

  /** The per-row error surfaced when a daemon rejects an action. */
  rowError: (model: ModelRef, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(modelsRowError(model.daemonInstanceId, model.providerId, model.modelId), {
      timeout: 5000,
      ...options,
    }),

  /** Click Load on a model row. */
  loadModel(model: ModelRef) {
    modelsScreenPage.loadButton(model).click();
  },

  /** Click Unload on a model row. */
  unloadModel(model: ModelRef) {
    modelsScreenPage.unloadButton(model).click();
  },

  /** Open the ACP chat for a model row. */
  openChat(model: ModelRef) {
    modelsScreenPage.chatButton(model).click();
  },

  /** Open the create-assistant dialog seeded from a model row. */
  openCreateAssistant(model: ModelRef) {
    byTestId(
      modelsRowCreateAssistant(model.daemonInstanceId, model.providerId, model.modelId),
    ).click();
  },

  /** The error row rendered for a daemon whose registry could not be read. */
  daemonError: (daemonInstanceId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(modelsDaemonError(daemonInstanceId), { timeout: 5000, ...options }),

  // ---------------------------------------------------------------------------
  // Providers
  // ---------------------------------------------------------------------------

  /** The providers panel. */
  providersPanel: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.modelsProvidersPanel, { timeout: 5000, ...options }),

  /** One provider row, on the daemon that owns it. */
  providerRow: (provider: ProviderRef, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(modelsProviderRow(provider.daemonInstanceId, provider.providerId), {
      timeout: 5000,
      ...options,
    }),

  /** The inline enumeration error rendered against a failing provider. */
  providerError: (provider: ProviderRef, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(modelsProviderError(provider.daemonInstanceId, provider.providerId), {
      timeout: 5000,
      ...options,
    }),

  /** Whether the daemon reports a stored credential for a provider, from `data-has-credential`. */
  providerHasCredential: (provider: ProviderRef): Cypress.Chainable<string> =>
    byTestId(modelsProviderCredential(provider.daemonInstanceId, provider.providerId), {
      timeout: 5000,
    })
      .invoke("attr", "data-has-credential")
      .then((value) =>
        requiredAttribute(value, "data-has-credential", `provider row ${provider.providerId}`),
      ),

  /** Re-enumerate a provider's models. */
  refreshProvider(provider: ProviderRef) {
    byTestId(modelsProviderRefresh(provider.daemonInstanceId, provider.providerId)).click();
  },

  /** The error the add-provider form reports when the provider could not be created. */
  addProviderError: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.modelsAddProviderError, { timeout: 5000, ...options }),

  /** Open the add-provider form. */
  openAddProviderForm() {
    byTestId(TEST_IDS.modelsAddProviderToggle).click();
  },

  /**
   * Fill and submit the add-provider form. Every field is required — a partial variant would
   * silently skip fields and let an incomplete form look like a successful submission.
   */
  fillAndSubmitAddProviderForm(provider: {
    kind: string;
    label: string;
    baseUrl: string;
    apiKey: string;
  }) {
    byTestId(TEST_IDS.modelsAddProviderKind).select(provider.kind);
    byTestId(TEST_IDS.modelsAddProviderLabel).clear().type(provider.label);
    byTestId(TEST_IDS.modelsAddProviderBaseUrl).clear().type(provider.baseUrl);
    byTestId(TEST_IDS.modelsAddProviderApiKey).clear().type(provider.apiKey);
    byTestId(TEST_IDS.modelsAddProviderSubmit).click();
  },

  // ---------------------------------------------------------------------------
  // Assistants
  // ---------------------------------------------------------------------------

  /** The assistants panel. */
  assistantsPanel: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.modelsAssistantsPanel, { timeout: 5000, ...options }),

  /** One assistant row, keyed by its `--agent` name. */
  assistantRow: (name: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(modelsAssistantRow(name), { timeout: 5000, ...options }),

  /** The tools assigned to an assistant, read from `data-assistant-tools`. */
  assistantTools: (name: string): Cypress.Chainable<string[]> =>
    byTestId(modelsAssistantTools(name), { timeout: 5000 })
      .invoke("attr", "data-assistant-tools")
      .then((value) =>
        attributeList(
          requiredAttribute(value, "data-assistant-tools", `assistant row ${name}`),
        ),
      ),

  /** The create-assistant dialog. */
  createAssistantDialog: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.modelsCreateAssistantDialog, { timeout: 5000, ...options }),

  /** The tool names offered by the create-assistant dialog, in DOM order. */
  assignableToolNames: (): Cypress.Chainable<string[]> =>
    modelsScreenPage
      .createAssistantDialog()
      .find("[data-testid^='models-create-assistant-tool-']")
      .then(($els) =>
        [...$els].map((el) =>
          el.getAttribute("data-testid")!.replace("models-create-assistant-tool-", ""),
        ),
      ),

  /** Fill and submit the create-assistant dialog with the given tool selection. */
  fillAndSubmitCreateAssistantForm(assistant: {
    name: string;
    label: string;
    systemPrompt: string;
    tools: string[];
  }) {
    byTestId(TEST_IDS.modelsCreateAssistantName).clear().type(assistant.name);
    byTestId(TEST_IDS.modelsCreateAssistantLabel).clear().type(assistant.label);
    byTestId(TEST_IDS.modelsCreateAssistantSystemPrompt).clear().type(assistant.systemPrompt);
    assistant.tools.forEach((tool) => byTestId(modelsCreateAssistantTool(tool)).check());
    byTestId(TEST_IDS.modelsCreateAssistantSubmit).click();
  },

  // ---------------------------------------------------------------------------
  // Chat
  // ---------------------------------------------------------------------------

  /** The ACP chat dialog. */
  chatDialog: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.modelsChatDialog, { timeout: 5000, ...options }),

  /** The chat transcript. */
  chatTranscript: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.modelsChatTranscript, { timeout: 5000, ...options }),

  /** Type a prompt into the chat and send it. */
  sendChatPrompt(prompt: string) {
    byTestId(TEST_IDS.modelsChatInput).clear().type(prompt);
    byTestId(TEST_IDS.modelsChatSend).click();
  },
};
