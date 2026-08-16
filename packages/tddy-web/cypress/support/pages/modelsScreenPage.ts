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
  modelsAssistantChat,
  modelsAssistantDelete,
  modelsAssistantEdit,
  modelsAssistantError,
  modelsAssistantRow,
  modelsAssistantsDaemonError,
  modelsAssistantTools,
  modelsChatMessage,
  modelsChatToolStatus,
  modelsChatWorkspaceOption,
  modelsCreateAssistantTool,
  modelsDaemonError,
  modelsEditAssistantTool,
  modelsProviderActionError,
  modelsProviderCredential,
  modelsProviderDelete,
  modelsProviderError,
  modelsProviderRefresh,
  modelsProviderRow,
  modelsProvidersDaemonError,
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

/**
 * Identifies one assistant row across the merged, cross-daemon panel. Assistant names are unique
 * per daemon, so two hosts may each define a `reviewer`.
 */
export interface AssistantRef {
  daemonInstanceId: string;
  name: string;
}

const rowId = (m: ModelRef) => modelsRow(m.daemonInstanceId, m.providerId, m.modelId);

/** The `data-testid` stem every tool checkbox in the edit-assistant dialog shares. */
const EDIT_ASSISTANT_TOOL_PREFIX = modelsEditAssistantTool("");

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

  /** Remove a provider from the daemon that owns it. */
  deleteProvider(provider: ProviderRef) {
    byTestId(modelsProviderDelete(provider.daemonInstanceId, provider.providerId)).click();
  },

  /** Why a write against a provider was refused, rendered against that provider's row. */
  providerActionError: (provider: ProviderRef, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(modelsProviderActionError(provider.daemonInstanceId, provider.providerId), {
      timeout: 5000,
      ...options,
    }),

  /** The failure the providers panel renders for a daemon whose registry could not be read. */
  providersDaemonError: (daemonInstanceId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(modelsProvidersDaemonError(daemonInstanceId), { timeout: 5000, ...options }),

  /** Why the providers panel has no rows, read from `data-registry-status`. */
  providersEmptyStatus: (): Cypress.Chainable<string> =>
    byTestId(TEST_IDS.modelsProvidersEmpty, { timeout: 5000 })
      .invoke("attr", "data-registry-status")
      .then((value) =>
        requiredAttribute(value, "data-registry-status", "the providers panel's empty state"),
      ),

  /** The row the providers panel shows in place of providers. */
  providersEmptyState: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.modelsProvidersEmpty, { timeout: 5000, ...options }),

  /** The error the add-provider form reports when the provider could not be created. */
  addProviderError: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.modelsAddProviderError, { timeout: 5000, ...options }),

  /** The daemon the add-provider form says a new provider will be created on. */
  addProviderTarget: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.modelsAddProviderTarget, { timeout: 5000, ...options }),

  /** Open the add-provider form. */
  openAddProviderForm() {
    byTestId(TEST_IDS.modelsAddProviderToggle).click();
  },

  /**
   * Fill the add-provider form. Every field is required — a partial variant would silently skip
   * fields and let an incomplete form look like a successful submission.
   */
  fillAddProviderForm(provider: { kind: string; label: string; baseUrl: string; apiKey: string }) {
    byTestId(TEST_IDS.modelsAddProviderKind).select(provider.kind);
    byTestId(TEST_IDS.modelsAddProviderLabel).clear().type(provider.label);
    byTestId(TEST_IDS.modelsAddProviderBaseUrl).clear().type(provider.baseUrl);
    byTestId(TEST_IDS.modelsAddProviderApiKey).clear().type(provider.apiKey);
  },

  /** The add-provider form's submit control. */
  addProviderSubmit: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.modelsAddProviderSubmit, { timeout: 5000, ...options }),

  /** Fill and submit the add-provider form. */
  fillAndSubmitAddProviderForm(provider: {
    kind: string;
    label: string;
    baseUrl: string;
    apiKey: string;
  }) {
    modelsScreenPage.fillAddProviderForm(provider);
    byTestId(TEST_IDS.modelsAddProviderSubmit).click();
  },

  // ---------------------------------------------------------------------------
  // Assistants
  // ---------------------------------------------------------------------------

  /** The assistants panel. */
  assistantsPanel: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.modelsAssistantsPanel, { timeout: 5000, ...options }),

  /** One assistant row, on the daemon that owns it. */
  assistantRow: (assistant: AssistantRef, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(modelsAssistantRow(assistant.daemonInstanceId, assistant.name), {
      timeout: 5000,
      ...options,
    }),

  /** The tools assigned to an assistant, read from `data-assistant-tools`. */
  assistantTools: (assistant: AssistantRef): Cypress.Chainable<string[]> =>
    byTestId(modelsAssistantTools(assistant.daemonInstanceId, assistant.name), { timeout: 5000 })
      .invoke("attr", "data-assistant-tools")
      .then((value) =>
        attributeList(
          requiredAttribute(value, "data-assistant-tools", `assistant row ${assistant.name}`),
        ),
      ),

  /** Why a write against an assistant was refused, rendered against that assistant's row. */
  assistantError: (assistant: AssistantRef, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(modelsAssistantError(assistant.daemonInstanceId, assistant.name), {
      timeout: 5000,
      ...options,
    }),

  /** The failure the assistants panel renders for a daemon whose registry could not be read. */
  assistantsDaemonError: (daemonInstanceId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(modelsAssistantsDaemonError(daemonInstanceId), { timeout: 5000, ...options }),

  /** Why the assistants panel has no rows, read from `data-registry-status`. */
  assistantsEmptyStatus: (): Cypress.Chainable<string> =>
    byTestId(TEST_IDS.modelsAssistantsEmpty, { timeout: 5000 })
      .invoke("attr", "data-registry-status")
      .then((value) =>
        requiredAttribute(value, "data-registry-status", "the assistants panel's empty state"),
      ),

  /** The row the assistants panel shows in place of assistants. */
  assistantsEmptyState: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.modelsAssistantsEmpty, { timeout: 5000, ...options }),

  /** Remove an assistant from the daemon that owns it. */
  deleteAssistant(assistant: AssistantRef) {
    byTestId(modelsAssistantDelete(assistant.daemonInstanceId, assistant.name)).click();
  },

  /** Open the edit dialog for an assistant. */
  openEditAssistant(assistant: AssistantRef) {
    byTestId(modelsAssistantEdit(assistant.daemonInstanceId, assistant.name)).click();
  },

  /** The Chat action on an assistant row. */
  assistantChatButton: (assistant: AssistantRef, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(modelsAssistantChat(assistant.daemonInstanceId, assistant.name), {
      timeout: 5000,
      ...options,
    }),

  /** Open the ACP chat with an assistant. */
  openAssistantChat(assistant: AssistantRef) {
    modelsScreenPage.assistantChatButton(assistant).click();
  },

  /** The create-assistant dialog. */
  createAssistantDialog: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.modelsCreateAssistantDialog, { timeout: 5000, ...options }),

  /** The edit-assistant dialog. */
  editAssistantDialog: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.modelsEditAssistantDialog, { timeout: 5000, ...options }),

  /** The label the edit-assistant dialog opened on. */
  editAssistantLabelField: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.modelsEditAssistantLabel, { timeout: 5000, ...options }),

  /** The system prompt the edit-assistant dialog opened on. */
  editAssistantSystemPromptField: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.modelsEditAssistantSystemPrompt, { timeout: 5000, ...options }),

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

  /**
   * How the create-assistant dialog knows the daemon's exec catalog, read from
   * `data-tool-catalog-status` — `loading`, `unavailable` or `ready`.
   */
  createAssistantToolCatalogStatus: (): Cypress.Chainable<string> =>
    byTestId(TEST_IDS.modelsCreateAssistantTools, { timeout: 5000 })
      .invoke("attr", "data-tool-catalog-status")
      .then((value) =>
        requiredAttribute(value, "data-tool-catalog-status", "the create-assistant tool picker"),
      ),

  /** The tool fieldset of the create-assistant dialog. */
  createAssistantTools: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.modelsCreateAssistantTools, { timeout: 5000, ...options }),

  /** The create-assistant dialog's submit control. */
  createAssistantSubmit: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.modelsCreateAssistantSubmit, { timeout: 5000, ...options }),

  /** Fill the create-assistant dialog with the given tool selection, without submitting. */
  fillCreateAssistantForm(assistant: {
    name: string;
    label: string;
    systemPrompt: string;
    tools: string[];
  }) {
    byTestId(TEST_IDS.modelsCreateAssistantName).clear().type(assistant.name);
    byTestId(TEST_IDS.modelsCreateAssistantLabel).clear().type(assistant.label);
    byTestId(TEST_IDS.modelsCreateAssistantSystemPrompt).clear().type(assistant.systemPrompt);
    assistant.tools.forEach((tool) => byTestId(modelsCreateAssistantTool(tool)).check());
  },

  /** Fill and submit the create-assistant dialog with the given tool selection. */
  fillAndSubmitCreateAssistantForm(assistant: {
    name: string;
    label: string;
    systemPrompt: string;
    tools: string[];
  }) {
    modelsScreenPage.fillCreateAssistantForm(assistant);
    byTestId(TEST_IDS.modelsCreateAssistantSubmit).click();
  },

  /**
   * Fill and submit the edit-assistant dialog. `tools` is the assistant's full tool set *after* the
   * edit: every box the dialog offers is set from it, so a tool left out is one the operator
   * unticked rather than one this helper quietly left as it found it.
   */
  fillAndSubmitEditAssistantForm(assistant: {
    label: string;
    systemPrompt: string;
    tools: string[];
  }) {
    byTestId(TEST_IDS.modelsEditAssistantLabel).clear().type(assistant.label);
    byTestId(TEST_IDS.modelsEditAssistantSystemPrompt).clear().type(assistant.systemPrompt);
    modelsScreenPage
      .editAssistantDialog()
      .find(`[data-testid^='${EDIT_ASSISTANT_TOOL_PREFIX}']`)
      .each(($box) => {
        const tool = $box.attr("data-testid")!.replace(EDIT_ASSISTANT_TOOL_PREFIX, "");
        cy.wrap($box).then(($el) =>
          assistant.tools.includes(tool) ? cy.wrap($el).check() : cy.wrap($el).uncheck(),
        );
      });
    byTestId(TEST_IDS.modelsEditAssistantSubmit).click();
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

  /** Why a chat prompt was not sent, or what the stream reported. */
  chatError: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.modelsChatError, { timeout: 5000, ...options }),

  /** One bubble of the chat transcript, by position. */
  chatMessage: (index: number, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(modelsChatMessage(index), { timeout: 5000, ...options }),

  /** What kind of bubble one transcript entry is, read from `data-message-kind`. */
  chatMessageKind: (index: number): Cypress.Chainable<string> =>
    modelsScreenPage
      .chatMessage(index)
      .invoke("attr", "data-message-kind")
      .then((value) => requiredAttribute(value, "data-message-kind", `chat message ${index}`)),

  /** The outcome marker on a transcript entry — absent while the entry reports no outcome. */
  chatToolStatusMarker: (index: number, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(modelsChatToolStatus(index), { timeout: 5000, ...options }),

  /** How a tool call ended, read from its marker's `data-tool-status`. */
  chatToolStatus: (index: number): Cypress.Chainable<string> =>
    modelsScreenPage
      .chatToolStatusMarker(index)
      .invoke("attr", "data-tool-status")
      .then((value) => requiredAttribute(value, "data-tool-status", `chat message ${index}`)),

  /** The workspace the open chat runs its assistant's tools in. */
  chatWorkspace: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.modelsChatWorkspace, { timeout: 5000, ...options }),

  /** Type a prompt into the chat and send it. */
  sendChatPrompt(prompt: string) {
    byTestId(TEST_IDS.modelsChatInput).clear().type(prompt);
    byTestId(TEST_IDS.modelsChatSend).click();
  },

  // ---------------------------------------------------------------------------
  // Choosing where an assistant's tools run
  // ---------------------------------------------------------------------------

  /** The dialog asking where a tool-bearing assistant's tools should run. */
  workspaceDialog: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.modelsChatWorkspaceDialog, { timeout: 5000, ...options }),

  /**
   * The workspace paths the owning daemon offers, in DOM order, read from `data-workspace-path` —
   * so the assertion is on the path that will be sent, not on the label rendered beside it.
   */
  offeredWorkspaces: (): Cypress.Chainable<string[]> =>
    modelsScreenPage
      .workspaceDialog()
      .find("[data-workspace-path]")
      .then(($els) =>
        [...$els].map((el) =>
          requiredAttribute(
            el.getAttribute("data-workspace-path") ?? undefined,
            "data-workspace-path",
            "a workspace option",
          ),
        ),
      ),

  /** Choose the workspace a project's main checkout provides. */
  chooseWorkspace(projectId: string) {
    byTestId(modelsChatWorkspaceOption(projectId)).click();
  },

  /** Why no workspace can be offered, read from `data-workspace-status`. */
  workspaceEmptyStatus: (): Cypress.Chainable<string> =>
    byTestId(TEST_IDS.modelsChatWorkspaceEmpty, { timeout: 5000 })
      .invoke("attr", "data-workspace-status")
      .then((value) =>
        requiredAttribute(value, "data-workspace-status", "the workspace picker's empty state"),
      ),

  /** The row the workspace picker shows in place of workspaces. */
  workspaceEmptyState: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.modelsChatWorkspaceEmpty, { timeout: 5000, ...options }),

  /** Why the owning daemon's projects could not be read. */
  workspaceError: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.modelsChatWorkspaceError, { timeout: 5000, ...options }),

  /** Leave the workspace choice without opening a chat. */
  cancelWorkspaceChoice() {
    byTestId(TEST_IDS.modelsChatWorkspaceCancel).click();
  },

  // ---------------------------------------------------------------------------
  // Dismissing a dialog
  // ---------------------------------------------------------------------------

  /** Press Escape, the way an operator leaves any other tddy-web modal. */
  pressEscape() {
    cy.get("body").type("{esc}");
  },

  /**
   * Press on the backdrop behind a dialog — the overlay that is the dialog panel's parent. Dispatched
   * on the overlay itself, since "outside the dialog" is what the component reads from the event's
   * target; the same way `VncOverlayAcceptance` presses its backdrop.
   */
  pressBackdropOf(dialog: () => Cypress.Chainable<JQuery<HTMLElement>>) {
    dialog()
      .parent()
      .then(($overlay) => {
        $overlay[0].dispatchEvent(new MouseEvent("mousedown", { bubbles: true, cancelable: true }));
      });
  },

  /** Press on the create-assistant dialog itself, the way a drag across one of its fields starts. */
  pressInsideCreateAssistantDialog() {
    modelsScreenPage.createAssistantDialog().then(($dialog) => {
      $dialog[0].dispatchEvent(new MouseEvent("mousedown", { bubbles: true, cancelable: true }));
    });
  },
};
