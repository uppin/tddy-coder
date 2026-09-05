import { useMemo, useState } from "react";
import { create } from "@bufbuild/protobuf";
import {
  AcpService,
  ModelSessionTargetSchema,
} from "../../gen/tddy/acp/v1/acp_pb";
import { useAuthContext } from "../../hooks/authProvider";
import { useHostConnection } from "../../rpc/connections/registry";
import { useDaemonClientFor } from "../../rpc/selectedDaemon";
import { toolStatusClass } from "../chat/chatEntryPresentation";
import { useAcpSessionOverClient } from "../chat/useAcpSession";
import { ModelsDialogShell } from "./ModelsDialogShell";
import type { RegistryChatTarget } from "../../utils/registryChatTarget";

/**
 * Chat with one model or assistant over the **existing** ACP stream (`AcpService.Session`) the
 * pr-stack chat already speaks — the provider-backed agent lives on the daemon, so the session is
 * addressed to the row's owning daemon rather than the selected one, and the transport is
 * `useAcpSession`'s, not a second chat implementation
 * (docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md § AC10).
 *
 * The daemon's ACP surface serves the whole registry from one service, so the handshake names which
 * row this session speaks as, the workspace an assistant's tools may run in, and the token that
 * authorizes reading the provider's credential (`NewSessionRequest.model_target` + `cwd`).
 *
 * The session is named to `useAcpSession` as the host that serves it, so a prompt sent after that
 * daemon has stopped being reachable is refused. Without the peer, a send onto a stream nobody reads
 * reports success and echoes the operator's own words back at them.
 */
export function ModelChatDialog({
  chat: target,
  onClose,
}: {
  chat: RegistryChatTarget;
  onClose: () => void;
}) {
  const client = useDaemonClientFor(AcpService, target.daemonInstanceId);
  const connection = useHostConnection(target.daemonInstanceId);
  const { sessionToken } = useAuthContext();
  const registry = useMemo(
    () => ({
      target: create(ModelSessionTargetSchema, {
        sessionToken: sessionToken ?? "",
        providerId: target.providerId,
        modelId: target.modelId,
        assistantId: target.assistantId,
      }),
      cwd: target.cwd,
    }),
    [sessionToken, target.providerId, target.modelId, target.assistantId, target.cwd],
  );
  // The owning daemon's own connection is this chat's liveness signal: it is read at the moment of
  // a send, so a host that dropped out between opening the chat and typing into it refuses the
  // prompt rather than enqueueing it onto a stream nobody is reading.
  const peer = useMemo(
    () => ({
      name: target.daemonInstanceId,
      isServing: () => connection?.status === "connected",
    }),
    [connection, target.daemonInstanceId],
  );
  const chat = useAcpSessionOverClient(client, undefined, peer, registry);
  const [draft, setDraft] = useState("");

  const send = () => {
    if (draft.trim() === "") return;
    if (chat.sendPrompt(draft)) setDraft("");
  };

  const error = chat.streamError ?? chat.workflowError ?? chat.sendError;

  return (
    <ModelsDialogShell
      testId="models-chat-dialog"
      label={`Chat with ${target.label}`}
      className="flex h-[32rem] w-full max-w-2xl flex-col gap-2 rounded-md border border-border bg-background p-4 text-sm text-foreground"
      onClose={onClose}
    >
      <div className="flex items-center gap-2">
        <h2 className="text-sm font-semibold">{target.label}</h2>
        <span className="text-xs text-muted-foreground">{target.modelId}</span>
        <span className="text-xs text-muted-foreground">{target.daemonInstanceId}</span>
        {target.cwd ? (
          <span data-testid="models-chat-workspace" className="text-xs text-muted-foreground">
            {target.cwd}
          </span>
        ) : null}
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
        {chat.messages.map((message, index) => (
          <div
            key={message.key}
            data-testid={`models-chat-message-${index}`}
            data-message-kind={message.from}
            className="whitespace-pre-wrap"
          >
            <span className="mr-2 text-xs text-muted-foreground">{message.from}</span>
            {/* A tool call's marker: until its update arrives, "it ran", "it is running" and "it
                failed" are the same sentence. */}
            {message.toolStatus ? (
              <span
                data-testid={`models-chat-message-${index}-tool-status`}
                data-tool-status={message.toolStatus}
                className={`mr-2 ${toolStatusClass(message.toolStatus)}`}
              >
                {message.toolStatus}
              </span>
            ) : null}
            <span>{message.text}</span>
          </div>
        ))}
      </div>

      {error ? (
        <div data-testid="models-chat-error" className="text-xs text-destructive">
          {error}
        </div>
      ) : null}

      <div className="flex items-center gap-2">
        <input
          data-testid="models-chat-input"
          placeholder={`Message ${target.modelId}…`}
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
    </ModelsDialogShell>
  );
}
