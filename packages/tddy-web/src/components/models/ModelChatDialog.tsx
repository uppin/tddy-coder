import { useMemo, useState } from "react";
import { create } from "@bufbuild/protobuf";
import {
  AcpService,
  ModelSessionTargetSchema,
} from "../../gen/tddy/acp/v1/acp_pb";
import { useAuthContext } from "../../hooks/authProvider";
import { useDaemonClientFor } from "../../rpc/selectedDaemon";
import type { ModelRow } from "../../utils/mergeRegistryEntries";
import { useAcpSessionOverClient } from "../chat/useAcpSession";

/**
 * Chat with one model over the **existing** ACP stream (`AcpService.Session`) the pr-stack chat
 * already speaks — the provider-backed agent lives on the daemon, so the session is addressed to
 * the model's owning daemon rather than the selected one, and the transport is `useAcpSession`'s,
 * not a second chat implementation
 * (docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md § AC10).
 *
 * The daemon's ACP surface serves the whole registry from one service, so the handshake names which
 * model this session speaks as and carries the token that authorizes reading its provider
 * credential (`NewSessionRequest.model_target`).
 */
export function ModelChatDialog({ model, onClose }: { model: ModelRow; onClose: () => void }) {
  const client = useDaemonClientFor(AcpService, model.daemonInstanceId);
  const { sessionToken } = useAuthContext();
  const modelTarget = useMemo(
    () =>
      create(ModelSessionTargetSchema, {
        sessionToken: sessionToken ?? "",
        providerId: model.providerId,
        modelId: model.modelId,
      }),
    [sessionToken, model.providerId, model.modelId],
  );
  const chat = useAcpSessionOverClient(client, undefined, undefined, modelTarget);
  const [draft, setDraft] = useState("");

  const send = () => {
    if (draft.trim() === "") return;
    if (chat.sendPrompt(draft)) setDraft("");
  };

  const error = chat.streamError ?? chat.workflowError ?? chat.sendError;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-background/80 p-4">
      <div
        data-testid="models-chat-dialog"
        role="dialog"
        aria-label={`Chat with ${model.label}`}
        className="flex h-[32rem] w-full max-w-2xl flex-col gap-2 rounded-md border border-border bg-background p-4 text-sm text-foreground"
      >
        <div className="flex items-center gap-2">
          <h2 className="text-sm font-semibold">{model.label}</h2>
          <span className="text-xs text-muted-foreground">{model.modelId}</span>
          <span className="text-xs text-muted-foreground">{model.daemonInstanceId}</span>
          <div className="flex-1" />
          <button
            type="button"
            data-testid="models-chat-close"
            className="rounded-md border border-input px-2 py-1 text-xs font-medium text-foreground hover:bg-accent"
            onClick={onClose}
          >
            Close
          </button>
        </div>

        <div
          data-testid="models-chat-transcript"
          className="flex flex-1 flex-col gap-1 overflow-auto rounded border border-border p-2"
        >
          {chat.messages.map((message) => (
            <div key={message.key} className="whitespace-pre-wrap">
              <span className="mr-2 text-xs text-muted-foreground">{message.from}</span>
              <span>{message.text}</span>
            </div>
          ))}
        </div>

        {error ? <div className="text-xs text-destructive">{error}</div> : null}

        <div className="flex items-center gap-2">
          <input
            data-testid="models-chat-input"
            placeholder={`Message ${model.modelId}…`}
            className="flex-1 rounded border border-input bg-background px-2 py-1 text-sm text-foreground"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                send();
              }
            }}
          />
          <button
            type="button"
            data-testid="models-chat-send"
            className="rounded-md border border-input px-3 py-1 text-sm font-medium text-foreground hover:bg-accent"
            onClick={send}
          >
            Send
          </button>
        </div>
      </div>
    </div>
  );
}
