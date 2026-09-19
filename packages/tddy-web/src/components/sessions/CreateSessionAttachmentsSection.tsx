import type { Client } from "@connectrpc/connect";
import type { SessionService } from "../../gen/session_pb";
import type { ProjectEntry } from "../../gen/project_pb";
import type { SessionFilesService } from "../../gen/session_files_pb";
import type { WorktreeService } from "../../gen/worktree_pb";
import type { SessionAttachments } from "../../hooks/useSessionAttachments";
import { AttachmentDropZone } from "./attachments/AttachmentDropZone";
import { HostDocumentPicker } from "./attachments/HostDocumentPicker";
import { SessionAttachmentList } from "./attachments/SessionAttachmentList";

export interface CreateSessionAttachmentsSectionProps {
  /** Everything `useSessionAttachments` answers — the rows, their progress and the pick handlers. */
  sessionAttachments: SessionAttachments;
  client: Client<typeof SessionService>;
  sessionFilesClient: Client<typeof SessionFilesService>;
  worktreeClient: Client<typeof WorktreeService>;
  sessionToken: string;
  /** Whether a creation is in flight — the drop zone and the rows are frozen while it is. */
  submitting: boolean;
  projects: ProjectEntry[];
  projectId: string;
}

/**
 * Attachments — documents the daemon materializes into `artifacts/attachments/` before the agent
 * starts. Shown for every session type, because the daemon materializes them for all of them.
 * See docs/ft/coder/session-attachments.md.
 *
 * Presentational: the rows and every handler are `useSessionAttachments`', passed through whole
 * rather than one prop each.
 */
export function CreateSessionAttachmentsSection({
  sessionAttachments,
  client,
  sessionFilesClient,
  worktreeClient,
  sessionToken,
  submitting,
  projects,
  projectId,
}: CreateSessionAttachmentsSectionProps) {
  const {
    attachments,
    progress: attachmentProgress,
    stagingDaemonInstanceId,
    problem: attachmentProblem,
    pickRefusal,
    hostDocPickerOpen,
    attachFiles,
    attachHostDocument,
    renameAttachment,
    removeAttachment,
    openHostDocPicker,
    closeHostDocPicker,
  } = sessionAttachments;
  return (
    <AttachmentDropZone
      onFilesPicked={attachFiles}
      onPickHostDocument={openHostDocPicker}
      disabled={submitting}
    >
      <SessionAttachmentList
        attachments={attachments}
        progress={attachmentProgress}
        onRename={renameAttachment}
        onRemove={removeAttachment}
        disabled={submitting}
      />
      {(attachmentProblem ?? pickRefusal) !== null && (
        <p data-testid="create-session-attachment-error" className="text-sm text-destructive">
          {attachmentProblem ?? pickRefusal}
        </p>
      )}
      {hostDocPickerOpen && (
        // `browsedDaemonInstanceId` must name the host `client` enumerates from, because every ref
        // the picker yields is stamped with it. It is `stagingDaemonInstanceId` because both derive
        // from the connected daemon — the host whose documents are listed is the host stamped. It is
        // deliberately NOT the session host: a document is read where it lives, and the session host
        // fetches it from there.
        <HostDocumentPicker
          client={client}
          sessionFilesClient={sessionFilesClient}
          worktreeClient={worktreeClient}
          sessionToken={sessionToken}
          browsedDaemonInstanceId={stagingDaemonInstanceId}
          project={projects.find((p) => p.projectId === projectId)}
          onPick={attachHostDocument}
          onClose={closeHostDocPicker}
        />
      )}
    </AttachmentDropZone>
  );
}
