import React from "react";
import type { Client } from "@connectrpc/connect";
import type { SessionService } from "tddy-client-web/gen/session_pb";
import type { ProjectService } from "tddy-client-web/gen/project_pb";
import type { CatalogService } from "tddy-client-web/gen/catalog_pb";
import type { SessionFilesService } from "tddy-client-web/gen/session_files_pb";
import type { WorktreeService } from "tddy-client-web/gen/worktree_pb";
import type { TaskService } from "tddy-client-web/gen/tasks_pb";
import { CreateSessionDialog, type CreateSessionInitialValues } from "tddy-session-create-web";

/**
 * The Start-session dialog of the PR-stack screen (`#orchestrators` 12/12, B11): the shared
 * creation form pre-filled from a planned node by `usePrStackStartSession`. Rendered only with
 * every client the dialog needs; without one the row never enters its starting state.
 */
export function PrStackStartSessionDialog(props: {
  open: boolean;
  client?: Client<typeof SessionService>;
  projectClient?: Client<typeof ProjectService>;
  catalogClient?: Client<typeof CatalogService>;
  sessionFilesClient?: Client<typeof SessionFilesService>;
  worktreeClient?: Client<typeof WorktreeService>;
  taskClient?: Client<typeof TaskService>;
  sessionToken: string;
  initialValues: CreateSessionInitialValues | undefined;
  onClose: () => void;
  onCreated: (sessionId: string) => void;
}): React.ReactElement | null {
  const {
    client,
    projectClient,
    catalogClient,
    sessionFilesClient,
    worktreeClient,
    taskClient,
    sessionToken,
    initialValues,
    onClose,
    onCreated,
  } = props;
  if (!client || !projectClient || !catalogClient || !sessionFilesClient || !worktreeClient) {
    return null;
  }
  return (
    <CreateSessionDialog
      open={props.open}
      client={client}
      projectClient={projectClient}
      catalogClient={catalogClient}
      sessionFilesClient={sessionFilesClient}
      worktreeClient={worktreeClient}
      taskClient={taskClient}
      sessionToken={sessionToken}
      initialValues={initialValues}
      onClose={onClose}
      onCreated={onCreated}
    />
  );
}
