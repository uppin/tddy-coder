/**
 * Centralised data-testid constants.
 *
 * Every `data-testid` value the package's Cypress specs use lives here — the raw string
 * appears once; tests use the named constant so a rename is a one-line change.
 *
 * For identifiers that include a dynamic segment (session ID, project ID, …) the
 * constant is a prefix or a helper function — see the examples below.
 */

import { safeTestIdPart } from "tddy-client-web";

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

/** Build a `cy.get` selector for `[data-testid='<id>']`. */
export const byTestId = (
  id: string,
  options?: Parameters<typeof cy.get>[1],
): Cypress.Chainable<JQuery<HTMLElement>> =>
  cy.get(`[data-testid='${id}']`, options);

// ---------------------------------------------------------------------------
// Auth / App shell
// ---------------------------------------------------------------------------

export const TEST_IDS = {
  // Worktrees
  shellMenuWorktrees: "shell-menu-worktrees",
  worktreesScreen: "worktrees-screen",
  worktreesProjectSelect: "worktrees-project-select",
  worktreeRow: "worktrees-row",
  worktreeRowName: "worktrees-row-name",
  worktreeRowSize: "worktrees-row-size",
  worktreeRowSpinner: "worktrees-row-spinner",
  worktreesRefresh: "worktrees-refresh",
  worktreesProgress: "worktrees-progress",
  worktreesSelectedName: "worktrees-selected-name",
  sessionWorktreeTab: "session-worktree-tab",
  sessionWorktreeSize: "session-worktree-size",
  sessionWorktreeDelete: "session-worktree-delete",
  sessionWorktreeDeleteConfirm: "session-worktree-delete-confirm",
  // CodexOAuth dialog
  codexOauthDialog: "codex-oauth-dialog",
  codexOauthDismiss: "codex-oauth-dismiss",
  codexOauthEmbeddingFallback: "codex-oauth-embedding-fallback",
  // Tasks drawer screen
  tasksDrawerScreen: "tasks-drawer-screen",
  tasksDrawer: "tasks-drawer",
  tasksOutputPane: "tasks-output-pane",
  tasksOutputPaneEmpty: "tasks-output-pane-empty",
  // Models & Agents screen (#/models)
  /** The Models & Agents screen root. */
  modelsScreen: "models-screen",
  /** The providers panel listing this fleet's configured providers. */
  modelsProvidersPanel: "models-providers-panel",
  /** Opens the add-provider form. */
  modelsAddProviderToggle: "models-add-provider-toggle",
  /** Add-provider form fields. */
  modelsAddProviderKind: "models-add-provider-kind",
  modelsAddProviderLabel: "models-add-provider-label",
  modelsAddProviderBaseUrl: "models-add-provider-base-url",
  modelsAddProviderApiKey: "models-add-provider-api-key",
  modelsAddProviderSubmit: "models-add-provider-submit",
  /** Names the daemon a newly added provider will be created on. */
  modelsAddProviderTarget: "models-add-provider-target",
  /** The error the add-provider form reports when the daemon refused (or was never addressed). */
  modelsAddProviderError: "models-add-provider-error",
  /** The row the table shows instead of models; carries `data-registry-status` for why. */
  modelsTableEmpty: "models-table-empty",
  /** The row the providers panel shows instead of providers; carries `data-registry-status`. */
  modelsProvidersEmpty: "models-providers-empty",
  /** The assistants panel. */
  modelsAssistantsPanel: "models-assistants-panel",
  /** The row the assistants panel shows instead of assistants; carries `data-registry-status`. */
  modelsAssistantsEmpty: "models-assistants-empty",
  /** Opens the create-assistant dialog for the focused model. */
  modelsCreateAssistantDialog: "models-create-assistant-dialog",
  modelsCreateAssistantName: "models-create-assistant-name",
  modelsCreateAssistantLabel: "models-create-assistant-label",
  modelsCreateAssistantSystemPrompt: "models-create-assistant-system-prompt",
  modelsCreateAssistantSubmit: "models-create-assistant-submit",
  /** The tool fieldset of the create-assistant dialog; carries `data-tool-catalog-status`. */
  modelsCreateAssistantTools: "models-create-assistant-tools",
  /**
   * The takeover fieldset of the create-assistant dialog — the *main agent's* tools this assistant
   * stands in for, which is a different question from the tools it may call itself.
   */
  modelsCreateAssistantReplacedTools: "models-create-assistant-replaced-tools",
  /** The edit-assistant dialog and its fields. */
  modelsEditAssistantDialog: "models-edit-assistant-dialog",
  modelsEditAssistantLabel: "models-edit-assistant-label",
  modelsEditAssistantSystemPrompt: "models-edit-assistant-system-prompt",
  modelsEditAssistantSubmit: "models-edit-assistant-submit",
  /** The tool fieldset of the edit-assistant dialog; carries `data-tool-catalog-status`. */
  modelsEditAssistantTools: "models-edit-assistant-tools",
  /** The takeover fieldset of the edit-assistant dialog. */
  modelsEditAssistantReplacedTools: "models-edit-assistant-replaced-tools",
  /** The ACP chat dialog opened from a model or assistant row. */
  modelsChatDialog: "models-chat-dialog",
  modelsChatInput: "models-chat-input",
  modelsChatSend: "models-chat-send",
  modelsChatTranscript: "models-chat-transcript",
  /** Why a chat prompt was not sent, or what the stream reported. */
  modelsChatError: "models-chat-error",
  /** The workspace a tool-bearing assistant's chat runs its tools in, once one is chosen. */
  modelsChatWorkspace: "models-chat-workspace",
  /** Chooses where a tool-bearing assistant's tools run, before its chat opens. */
  modelsChatWorkspaceDialog: "models-chat-workspace-dialog",
  /** Why no workspace can be offered; carries `data-workspace-status`. */
  modelsChatWorkspaceEmpty: "models-chat-workspace-empty",
  /** Why the owning daemon's projects could not be read. */
  modelsChatWorkspaceError: "models-chat-workspace-error",
  /** Leaves the workspace choice without opening a chat. */
  modelsChatWorkspaceCancel: "models-chat-workspace-cancel",
  // Projects screen (/projects)
  projectsScreen: "projects-screen",
  projectsList: "projects-list",
  projectsCreateProjectToggle: "projects-create-project-toggle",
  projectsCreateProjectForm: "projects-create-project-form",
  projectsNewProjectName: "projects-new-project-name",
  projectsNewProjectGitUrl: "projects-new-project-git-url",
  projectsNewProjectUserRelativePath: "projects-new-project-user-relative-path",
  projectsCreateProjectSubmit: "projects-create-project-submit",
  /** Why the last create failed — the daemon's reason, or that the host is not connected. */
  projectsCreateProjectError: "projects-create-project-error",
  /** Why the project list could not be loaded; never shown as an empty list. */
  projectsLoadError: "projects-load-error",
  /** Why the vault's accounts could not be read for the assignment pickers. */
  projectsAccountsLoadError: "projects-accounts-load-error",
  daemonSettingsLivekitUrl: "daemon-settings-livekit-url",
  daemonSettingsLivekitApiKey: "daemon-settings-livekit-api-key",
  /** Says whether the daemon holds a LiveKit API secret; never shows the secret itself. */
  daemonSettingsLivekitSecretState: "daemon-settings-livekit-secret-state",
  daemonSettingsLivekitStatus: "daemon-settings-livekit-status",
  daemonSettingsTelegramStatus: "daemon-settings-telegram-status",
  /** The operator's LiveKit switch: off preserves every other field in the block. */
  daemonSettingsLivekitEnabled: "daemon-settings-livekit-enabled",
  daemonSettingsSave: "daemon-settings-save",
  /** The fields an update persisted but could not apply to the running daemon. */
  daemonSettingsRestartRequired: "daemon-settings-restart-required",
  daemonSettingsError: "daemon-settings-error",
} as const;

/** Opens the ACP chat with an assistant — its model, its system prompt and its tools. */
export const modelsAssistantChat = (daemonInstanceId: string, name: string) =>
  `${modelsAssistantRow(daemonInstanceId, name)}-chat`;

/** Removes an assistant from the daemon that owns it. */
export const modelsAssistantDelete = (daemonInstanceId: string, name: string) =>
  `${modelsAssistantRow(daemonInstanceId, name)}-delete`;

/** Opens the edit dialog for an assistant. */
export const modelsAssistantEdit = (daemonInstanceId: string, name: string) =>
  `${modelsAssistantRow(daemonInstanceId, name)}-edit`;

/** Why a write against an assistant was refused. */
export const modelsAssistantError = (daemonInstanceId: string, name: string) =>
  `${modelsAssistantRow(daemonInstanceId, name)}-error`;

/**
 * One assistant row. An assistant's `--agent` name is unique *per daemon*, so two hosts may each
 * define a `reviewer` — the owning daemon is part of the id, or their rows would collide.
 */
export const modelsAssistantRow = (daemonInstanceId: string, name: string) =>
  `models-assistant-row-${daemonInstanceId}-${safeTestIdPart(name)}`;

/** The assigned-tools cell of an assistant row; carries `data-assistant-tools`. */
export const modelsAssistantTools = (daemonInstanceId: string, name: string) =>
  `${modelsAssistantRow(daemonInstanceId, name)}-tools`;

/** The failure row the assistants panel renders for a daemon whose registry could not be read. */
export const modelsAssistantsDaemonError = (daemonInstanceId: string) =>
  `models-assistants-daemon-error-${safeTestIdPart(daemonInstanceId)}`;

/** One rendered bubble of the models chat transcript; carries `data-message-kind`. */
export const modelsChatMessage = (index: number) => `models-chat-message-${index}`;

/** A tool bubble's status marker; carries `data-tool-status`. Absent on non-tool bubbles. */
export const modelsChatToolStatus = (index: number) => `${modelsChatMessage(index)}-tool-status`;

/** One workspace the owning daemon offers a tool-bearing assistant; carries `data-workspace-path`. */
export const modelsChatWorkspaceOption = (projectId: string) =>
  `models-chat-workspace-${safeTestIdPart(projectId)}`;

/**
 * One main-agent tool the create-assistant dialog offers to hand over to the assistant.
 *
 * Deliberately not a `models-create-assistant-tool-` id: the two pickers offer the same ten names,
 * and a shared stem would make a prefix selector for one of them match both.
 */
export const modelsCreateAssistantReplacedTool = (toolName: string) =>
  `models-create-assistant-replaced-tool-${toolName}`;

/** One selectable tool checkbox in the create-assistant dialog. */
export const modelsCreateAssistantTool = (toolName: string) =>
  `models-create-assistant-tool-${toolName}`;

/** The error row rendered for a daemon whose registry could not be read. */
export const modelsDaemonError = (daemonInstanceId: string) =>
  `models-daemon-error-${daemonInstanceId}`;

/** One main-agent tool the edit-assistant dialog offers to hand over to the assistant. */
export const modelsEditAssistantReplacedTool = (toolName: string) =>
  `models-edit-assistant-replaced-tool-${toolName}`;

/** One selectable tool checkbox in the edit-assistant dialog. */
export const modelsEditAssistantTool = (toolName: string) =>
  `models-edit-assistant-tool-${toolName}`;

/** Why a write against a provider was refused — held apart from its enumeration error. */
export const modelsProviderActionError = (daemonInstanceId: string, providerId: string) =>
  `${modelsProviderRow(daemonInstanceId, providerId)}-action-error`;

/** Whether a credential is stored for a provider — never the credential itself. */
export const modelsProviderCredential = (daemonInstanceId: string, providerId: string) =>
  `${modelsProviderRow(daemonInstanceId, providerId)}-credential`;

/** Removes a provider from the daemon that owns it. */
export const modelsProviderDelete = (daemonInstanceId: string, providerId: string) =>
  `${modelsProviderRow(daemonInstanceId, providerId)}-delete`;

/** The inline enumeration error rendered against a provider whose last refresh failed. */
export const modelsProviderError = (daemonInstanceId: string, providerId: string) =>
  `${modelsProviderRow(daemonInstanceId, providerId)}-error`;

/** Re-enumerates one provider's models from the provider itself. */
export const modelsProviderRefresh = (daemonInstanceId: string, providerId: string) =>
  `${modelsProviderRow(daemonInstanceId, providerId)}-refresh`;

/**
 * `[data-testid="models-provider-row-<daemonInstanceId>-<providerId>"]` — one row per configured
 * provider. Provider ids are minted per daemon (`prov-ollama` exists on every host), so the owning
 * daemon is part of the id; without it two hosts' rows would collide in the DOM.
 */
export const modelsProviderRow = (daemonInstanceId: string, providerId: string) =>
  `models-provider-row-${daemonInstanceId}-${providerId}`;

/** The failure row the providers panel renders for a daemon whose registry could not be read. */
export const modelsProvidersDaemonError = (daemonInstanceId: string) =>
  `models-providers-daemon-error-${safeTestIdPart(daemonInstanceId)}`;

/**
 * `[data-testid="models-row-<daemonInstanceId>-<providerId>-<modelId>"]` — one row per model.
 * A model id may contain a colon (`qwen3:32b`), so it goes through `safeTestIdPart`.
 */
export const modelsRow = (daemonInstanceId: string, providerId: string, modelId: string) =>
  `models-row-${daemonInstanceId}-${providerId}-${safeTestIdPart(modelId)}`;

/** The Chat action on a model row (rendered only for chat-capable models). */
export const modelsRowChat = (daemonInstanceId: string, providerId: string, modelId: string) =>
  `${modelsRow(daemonInstanceId, providerId, modelId)}-chat`;

/** The "create assistant from this model" action on a model row. */
export const modelsRowCreateAssistant = (
  daemonInstanceId: string,
  providerId: string,
  modelId: string,
) => `${modelsRow(daemonInstanceId, providerId, modelId)}-create-assistant`;

/** The owning-daemon cell of a model row. */
export const modelsRowDaemon = (daemonInstanceId: string, providerId: string, modelId: string) =>
  `${modelsRow(daemonInstanceId, providerId, modelId)}-daemon`;

/** The per-row error surfaced when an action is rejected by the daemon. */
export const modelsRowError = (daemonInstanceId: string, providerId: string, modelId: string) =>
  `${modelsRow(daemonInstanceId, providerId, modelId)}-error`;

/** The capability-label cell of a model row; carries `data-model-labels` for exact assertions. */
export const modelsRowLabels = (daemonInstanceId: string, providerId: string, modelId: string) =>
  `${modelsRow(daemonInstanceId, providerId, modelId)}-labels`;

/** The Load action on a model row (rendered only when the model is not loaded). */
export const modelsRowLoad = (daemonInstanceId: string, providerId: string, modelId: string) =>
  `${modelsRow(daemonInstanceId, providerId, modelId)}-load`;

/** The load-state cell of a model row; carries `data-load-state`. */
export const modelsRowLoadState = (daemonInstanceId: string, providerId: string, modelId: string) =>
  `${modelsRow(daemonInstanceId, providerId, modelId)}-load-state`;

/** The marker rendered on a model row whose provider's last enumeration failed. */
export const modelsRowStale = (daemonInstanceId: string, providerId: string, modelId: string) =>
  `${modelsRow(daemonInstanceId, providerId, modelId)}-stale`;

/** The Unload action on a model row (rendered only when the model is loaded). */
export const modelsRowUnload = (daemonInstanceId: string, providerId: string, modelId: string) =>
  `${modelsRow(daemonInstanceId, providerId, modelId)}-unload`;

/** `[data-testid="project-account-row-<projectId>-<provider>"]` — one provider's assignment row on
 *  a project card. Present for every provider the vault holds accounts at, assigned or not. */
export const projectAccountRow = (projectId: string, provider: string) =>
  `project-account-row-${projectId}-${provider}`;

/** `[data-testid="project-account-select-<projectId>-<provider>"]` — the account assigned to a
 *  project at one provider. Its empty-valued option is "no account assigned", which is a real
 *  choice and not a placeholder: an unassigned provider resolves to nothing. */
export const projectAccountSelect = (projectId: string, provider: string) =>
  `project-account-select-${projectId}-${provider}`;

/** `[data-testid="project-add-to-host-select-<projectId>"]` — target-host `<select>` */
export const projectAddToHostSelect = (projectId: string) =>
  `project-add-to-host-select-${projectId}`;

/** `[data-testid="project-add-to-host-submit-<projectId>"]` — submits the add-to-host action */
export const projectAddToHostSubmit = (projectId: string) =>
  `project-add-to-host-submit-${projectId}`;

/** `[data-testid="project-add-to-host-toggle-<projectId>"]` — opens the add-to-host control */
export const projectAddToHostToggle = (projectId: string) =>
  `project-add-to-host-toggle-${projectId}`;

/** `[data-testid="project-add-to-host-user-relative-path-<projectId>"]` — optional clone-location
 *  input in the add-to-host control (path relative to the target host's home). */
export const projectAddToHostUserRelativePath = (projectId: string) =>
  `project-add-to-host-user-relative-path-${projectId}`;

/** `[data-testid="project-card-<projectId>"]` — one card per logical project (may span hosts) */
export const projectCard = (projectId: string) => `project-card-${projectId}`;

/** `[data-testid="project-card-error-<projectId>"]` — why the card's last action or load failed */
export const projectCardError = (projectId: string) => `project-card-error-${projectId}`;

/** `[data-testid="project-card-id-<projectId>"]` — the logical project id shown on its card */
export const projectCardId = (projectId: string) => `project-card-id-${projectId}`;

/** `[data-testid="project-default-branch-select-<projectId>"]` — the project's default-branch
 *  (`main_branch_ref`) dropdown, listing the project's remote branches. */
export const projectDefaultBranchSelect = (projectId: string) =>
  `project-default-branch-select-${projectId}`;

/** `[data-testid="project-host-base-location-<daemonInstanceId>"]` — a daemon's advertised base
 *  clone location (repos_base_path), surfaced in the Projects screen. */
export const projectHostBaseLocation = (daemonInstanceId: string) =>
  `project-host-base-location-${daemonInstanceId}`;

/** `[data-testid="project-host-gh-<projectId>-<daemonInstanceId>"]` — on a Local gh project's host
 *  row: whether that host's `gh` is logged in for the project's GitHub host, and as whom. */
export const projectHostGh = (projectId: string, daemonInstanceId: string) =>
  `project-host-gh-${projectId}-${daemonInstanceId}`;

/** `[data-testid="project-host-row-<projectId>-<daemonInstanceId>"]` — one row per hosting daemon */
export const projectHostRow = (projectId: string, daemonInstanceId: string) =>
  `project-host-row-${projectId}-${daemonInstanceId}`;

/** `[data-testid="project-local-gh-toggle-<projectId>"]` — the checkbox that sets a project to take
 *  its GitHub identity from each host's own `gh` login instead of a vault account. */
export const projectLocalGhToggle = (projectId: string) => `project-local-gh-toggle-${projectId}`;

/** `[data-testid="tasks-channel-output-<taskId>-<channelId>"]` — channel output area */
export const tasksChannelOutput = (taskId: string, channelId: string) =>
  `tasks-channel-output-${taskId}-${channelId}`;

/** `[data-testid="tasks-channel-tab-<taskId>-<channelId>"]` — channel tab */
export const tasksChannelTab = (taskId: string, channelId: string) =>
  `tasks-channel-tab-${taskId}-${channelId}`;

/** `[data-testid="tasks-drawer-item-<taskId>"]` — clickable drawer row */
export const tasksDrawerItem = (taskId: string) => `tasks-drawer-item-${taskId}`;

/** `[data-testid="tasks-drawer-item-cancel-<taskId>"]` — inline Cancel button in drawer row */
export const tasksDrawerItemCancel = (taskId: string) => `tasks-drawer-item-cancel-${taskId}`;

/** `[data-testid="tasks-drawer-item-kind-<taskId>"]` — kind text */
export const tasksDrawerItemKind = (taskId: string) => `tasks-drawer-item-kind-${taskId}`;

/** `[data-testid="tasks-drawer-item-status-<taskId>"]` — status indicator dot */
export const tasksDrawerItemStatus = (taskId: string) => `tasks-drawer-item-status-${taskId}`;

/** `[data-testid="tasks-output-pane-cancel-<taskId>"]` — Cancel button in output pane */
export const tasksOutputPaneCancel = (taskId: string) => `tasks-output-pane-cancel-${taskId}`;

/** `[data-testid="tasks-output-pane-status-<taskId>"]` — status label in output pane */
export const tasksOutputPaneStatus = (taskId: string) => `tasks-output-pane-status-${taskId}`;
