import type React from "react";
import type { Client } from "@connectrpc/connect";
import type { SessionEntry } from "tddy-client-web/gen/session_pb";
import { WorkQueueService } from "tddy-client-web/gen/work_queue_pb";
import { useHostClient, useSelectedDaemon } from "tddy-client-web";
import type { ProjectService, ProjectEntry } from "tddy-client-web/gen/project_pb";
import type { CatalogService } from "tddy-client-web/gen/catalog_pb";
import type { TaskService } from "tddy-client-web/gen/tasks_pb";
import { WorkQueueView } from "tddy-workqueue-web";
import { SessionIndicatorDot } from "./SessionIndicatorDot";
import { SESSION_INDICATOR_DOT_STYLES } from "./sessionIndicatorDotStyles";

type WorkQueueViewProps = React.ComponentProps<typeof WorkQueueView>;

/** What the inspector's Work queue tab needs beyond the clients every inspector tab shares. */
export interface SessionWorkQueueTabInputs {
  /** The Work queue tab's Start-session dialog reads the project registry, the catalog and the start's
   *  task through these, on the same host as `client`. The dialog is offered only with all of them. */
  projectClient?: Client<typeof ProjectService>;
  catalogClient?: Client<typeof CatalogService>;
  taskClient?: Client<typeof TaskService>;
  /** The projects the drawer lists — the Work queue tab reads its project's default branch from them. */
  projects?: ReadonlyArray<ProjectEntry>;
  /** `work_queue.WorkQueueService` on the selected daemon — the Work queue tab's queue is read and edited through it. */
  workQueueClient?: Client<typeof WorkQueueService>;
  sessionMetadataBySessionId?: WorkQueueViewProps["sessionMetadataBySessionId"];
  onChildSessionStarted?: WorkQueueViewProps["onChildSessionStarted"];
}

export interface SessionWorkQueueTabProps extends SessionWorkQueueTabInputs {
  /** The session whose own work queue the tab shows. */
  session: SessionEntry;
  sessions: ReadonlyArray<SessionEntry>;
  sessionToken: string;
  onOpenSession?: WorkQueueViewProps["onOpenSession"];
  client: WorkQueueViewProps["client"];
  sessionFilesClient: WorkQueueViewProps["sessionFilesClient"];
  worktreeClient: WorkQueueViewProps["worktreeClient"];
}

/**
 * The inspector's Work queue tab (`#workqueue` 5/14): the session's own queue — its id is the
 * session's — shown in the queue view, addressed at the daemon that holds the session. The queue is
 * watched on that daemon's own connection (a session with no host named is the selected daemon's).
 */
export function SessionWorkQueueTab({
  session,
  projects = [],
  sessions,
  ...rest
}: SessionWorkQueueTabProps) {
  const { selectedInstanceId } = useSelectedDaemon();
  const watchClient = useHostClient(WorkQueueService, session.daemonInstanceId || selectedInstanceId);
  return (
    <>
    <style>{SESSION_INDICATOR_DOT_STYLES}</style>
    <WorkQueueView
      address={{
        projectId: session.projectId,
        queueId: session.sessionId,
        daemonInstanceId: session.daemonInstanceId,
      }}
      defaultBranch={
        projects.find((project) => project.projectId === session.projectId)?.mainBranchRef ?? ""
      }
      sessions={[...sessions]}
      watchClient={watchClient}
      // An attached session shows the activity dot the drawer paints for it: green and breathing while
      // it works, green when idle, yellow when it needs input, grey when disconnected.
      renderSessionIndicator={(sessionId) => {
        const attached = sessions.find((candidate) => candidate.sessionId === sessionId);
        return attached ? (
          <SessionIndicatorDot
            session={attached}
            sizeClassName="h-3 w-3"
            testId={`work-item-session-indicator-${sessionId}`}
          />
        ) : null;
      }}
      {...rest}
    />
    </>
  );
}
