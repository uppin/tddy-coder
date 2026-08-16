import { useCallback, useState } from "react";
import { ModelLoadState } from "../../gen/models_pb";
import { useSelectedDaemon } from "../../rpc/selectedDaemon";
import type { AssistantRow, ModelRow } from "../../utils/mergeRegistryEntries";
import { AppShell } from "../shell/AppShell";
import { ChatWorkspaceDialog } from "./ChatWorkspaceDialog";
import { ModelChatDialog } from "./ModelChatDialog";
import { ModelsScreen } from "./ModelsScreen";
import {
  chatWithAssistant,
  chatWithModel,
  needsWorkspace,
  type RegistryChatTarget,
} from "../../utils/registryChatTarget";
import { useModelRegistryFanOut } from "./useModelRegistryFanOut";

/**
 * Why a provider cannot be created while no daemon is selected. A provider belongs to exactly one
 * daemon, so there is no sensible host to send it to — and addressing the empty instance id instead
 * would report `no connection to daemon ` at an operator who never named one.
 */
const NO_DAEMON_SELECTED = "select a daemon before adding a provider";

/**
 * Data container for the Models & Agents screen (`/models`): the providers, models and assistants of
 * **every** daemon in the common room, merged into one page, with each per-row action routed back to
 * the daemon that owns that row (see {@link useModelRegistryFanOut}).
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md.
 */
export function ModelsAppPage({ onNavigate }: { onNavigate: (path: string) => void }) {
  const registry = useModelRegistryFanOut();
  const { selectedInstanceId } = useSelectedDaemon();
  const [chatTarget, setChatTarget] = useState<RegistryChatTarget | null>(null);
  /** The assistant whose chat is waiting on a workspace for its tools; `null` when none is. */
  const [assistantAwaitingWorkspace, setAssistantAwaitingWorkspace] = useState<AssistantRow | null>(
    null,
  );

  /**
   * Chatting with a model requires it to be resident, so a model that has been evicted is loaded
   * first and the chat opens on the answer. A load that fails leaves the row's error visible and no
   * chat opens — a conversation with a model that is not there would only fail later, less clearly.
   */
  const openChat = useCallback(
    async (model: ModelRow) => {
      if (model.loadState === ModelLoadState.NOT_LOADED && !(await registry.loadModel(model))) {
        return;
      }
      setChatTarget(chatWithModel(model));
    },
    [registry],
  );

  /**
   * An assistant with tools runs them somewhere, and the daemon only accepts a directory this
   * operator already owns — so the workspace is chosen before the stream opens rather than
   * discovered as a refused handshake. An assistant with no tools reaches no engine and is chatted
   * with straight away.
   */
  const openAssistantChat = useCallback((assistant: AssistantRow) => {
    if (needsWorkspace(assistant)) {
      setAssistantAwaitingWorkspace(assistant);
      return;
    }
    setChatTarget(chatWithAssistant(assistant, ""));
  }, []);

  return (
    <AppShell title="Models & Agents" onNavigate={onNavigate} variant="scroll">
      <ModelsScreen
        providers={registry.providers}
        models={registry.models}
        assistants={registry.assistants}
        failures={registry.failures}
        providerErrors={registry.providerErrors}
        providerActionErrors={registry.providerActionErrors}
        assistantErrors={registry.assistantErrors}
        modelErrors={registry.modelErrors}
        status={registry.status}
        toolsFor={registry.toolsFor}
        addProviderTarget={selectedInstanceId ?? ""}
        onAddProvider={(input) =>
          selectedInstanceId
            ? registry.createProvider({ daemonInstanceId: selectedInstanceId, ...input })
            : Promise.resolve(NO_DAEMON_SELECTED)
        }
        onRefreshProvider={(provider) => void registry.refreshProvider(provider)}
        onDeleteProvider={(provider) => void registry.deleteProvider(provider)}
        onLoadModel={(model) => void registry.loadModel(model)}
        onUnloadModel={(model) => void registry.unloadModel(model)}
        onOpenChat={(model) => void openChat(model)}
        onOpenAssistantChat={openAssistantChat}
        onCreateAssistant={({ model, name, label, systemPrompt, tools }) =>
          registry.createAssistant({
            daemonInstanceId: model.daemonInstanceId,
            name,
            label,
            providerId: model.providerId,
            modelId: model.modelId,
            systemPrompt,
            tools,
          })
        }
        onUpdateAssistant={(input) => registry.updateAssistant(input)}
        onDeleteAssistant={(assistant) => void registry.deleteAssistant(assistant)}
      />
      {assistantAwaitingWorkspace ? (
        <ChatWorkspaceDialog
          assistant={assistantAwaitingWorkspace}
          onChoose={(cwd) => {
            setChatTarget(chatWithAssistant(assistantAwaitingWorkspace, cwd));
            setAssistantAwaitingWorkspace(null);
          }}
          onClose={() => setAssistantAwaitingWorkspace(null)}
        />
      ) : null}
      {chatTarget ? (
        <ModelChatDialog chat={chatTarget} onClose={() => setChatTarget(null)} />
      ) : null}
    </AppShell>
  );
}
