import React from "react";
import type { Client } from "@connectrpc/connect";
import type { ConnectionService } from "../../gen/connection_pb";
import { Button } from "../ui/button";
import { CreateSessionPane, type CreateSessionInitialValues } from "./CreateSessionPane";

type ConnectionClient = Client<typeof ConnectionService>;

export interface CreateSessionDialogProps {
  open: boolean;
  client: ConnectionClient;
  sessionToken: string;
  onClose: () => void;
  onCreated: (sessionId: string) => void;
  initialValues?: CreateSessionInitialValues;
}

/**
 * Overlay dialog that hosts the shared {@link CreateSessionPane}. Used by the PR-stack "Start
 * session" flow to let the operator review and adjust the pre-filled fields (branch, prompt, stack
 * parent) before the child session spawns. Follows the same hand-rolled overlay pattern as
 * `CodexOAuthDialog` (no shadcn Dialog in this app).
 */
export function CreateSessionDialog({
  open,
  client,
  sessionToken,
  onClose,
  onCreated,
  initialValues,
}: CreateSessionDialogProps) {
  if (!open) {
    return null;
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"
      data-testid="create-session-dialog"
      role="dialog"
      aria-modal="true"
      aria-label="New session"
    >
      <div className="bg-card text-card-foreground border-border flex max-h-[90vh] w-full max-w-lg flex-col overflow-hidden rounded-xl border shadow-lg">
        <div className="border-border flex items-center justify-between border-b px-4 py-3">
          <h2 className="text-sm font-semibold">New session</h2>
          <Button
            type="button"
            variant="outline"
            size="sm"
            data-testid="create-session-dialog-close"
            onClick={onClose}
          >
            Close
          </Button>
        </div>

        <div className="min-h-0 flex-1 overflow-auto">
          <CreateSessionPane
            client={client}
            sessionToken={sessionToken}
            initialValues={initialValues}
            onCancel={onClose}
            onCreated={onCreated}
          />
        </div>
      </div>
    </div>
  );
}
