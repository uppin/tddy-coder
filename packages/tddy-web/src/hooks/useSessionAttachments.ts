/**
 * The whole attachment concern of the new-session form: the rows the operator has attached, what is
 * wrong with them, the host's advertised size cap, the upload of local files on submit, and the
 * streamed start that reports the host's materialization progress.
 *
 * One hook rather than inline state because the pieces are only meaningful together — a row's
 * refusal, its staged location and its progress are all keyed off the same set of rows, and the
 * staging host is what ties a staged ref to the bytes behind it. `CreateSessionPane` consumes this as
 * one surface and owns none of it.
 *
 * Changeset: `2026-08-01-session-attach-ui`
 * Feature: docs/ft/coder/session-attachments.md
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-01-session-attach-ui.md
 */

import { useEffect, useRef, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { MessageInitShape } from "@bufbuild/protobuf";
import type {
  ConnectionService,
  SessionAttachmentSchema,
  StartSessionRequestSchema,
  StartSessionResponse,
} from "../gen/connection_pb";
import {
  duplicateBasenames,
  validateAttachmentBasename,
  type AttachmentBasenameRejection,
} from "../lib/attachmentBasenames";
import { formatAttachmentBytes } from "../lib/attachmentBytes";
import { randomUuid } from "../lib/randomId";
import { useDaemons, useSelectedDaemon } from "../rpc/selectedDaemon";
import type { HostDocumentPick } from "../components/sessions/attachments/HostDocumentPicker";
import type {
  AttachmentProgress,
  AttachmentProgressByBasename,
  InitialAttachment,
  PendingAttachment,
} from "../components/sessions/attachments/pendingAttachment";
import {
  useStagedAttachmentUpload,
  type StagedAttachmentFile,
  type StagedAttachmentProgress,
} from "./useStagedAttachmentUpload";

/** The `StartSession` request both entry points (unary and streamed) are built as. */
export type StartSessionRequestInit = MessageInitShape<typeof StartSessionRequestSchema>;
export type SessionAttachmentInit = MessageInitShape<typeof SessionAttachmentSchema>;

export interface UseSessionAttachmentsArgs {
  client: Client<typeof ConnectionService>;
  sessionToken: string;
  /**
   * Host that will run the session. One half of the effective size cap: the session host re-checks an
   * attachment's size while fetching it, so its cap bounds the attachment just as the staging host's
   * does.
   */
  sessionDaemonInstanceId: string;
  /**
   * Rows the form opens with — the documents a caller already knows the session should carry (the
   * PR-stack Start-session dialog pre-attaches the node's own documents this way).
   *
   * Read once, when the hook first mounts: they seed the operator's rows rather than owning them, so
   * a row removed here stays removed however often the caller re-renders with the same list.
   */
  initialAttachments?: readonly InitialAttachment[];
}

/** Everything the form needs to render and submit its attachment rows. */
export interface SessionAttachments {
  attachments: PendingAttachment[];
  /** Per-row progress while a creation is in flight, keyed by basename. */
  progress: AttachmentProgressByBasename;
  /**
   * Host local files are uploaded to and that every staged ref is stamped with — the daemon this
   * form's client is connected to, which is not necessarily the host that runs the session.
   */
  stagingDaemonInstanceId: string;
  /** The first thing wrong with the current rows, or `null` when the daemon would accept them. */
  problem: string | null;
  /** Why a picked document could not be attached at all (over the cap), as opposed to a bad row. */
  pickRefusal: string | null;
  /** True while the host-document picker is open. */
  hostDocPickerOpen: boolean;
  attachFiles: (files: File[]) => void;
  attachHostDocument: (pick: HostDocumentPick) => void;
  renameAttachment: (id: string, basename: string) => void;
  removeAttachment: (id: string) => void;
  openHostDocPicker: () => void;
  closeHostDocPicker: () => void;
  /** Drops the progress of a previous attempt, so a resubmit does not show stale percentages. */
  resetProgress: () => void;
  /**
   * Uploads whatever is not already on the staging host and returns the whole attachment set in wire
   * form. A staging failure rejects, which fails the creation: a session must never reference a batch
   * that is missing bytes.
   */
  stageAttachments: () => Promise<SessionAttachmentInit[]>;
  /**
   * Runs one `StreamStartSession`, rendering the host's per-attachment materialization progress as it
   * arrives and resolving to the terminal result. `null` means the form unmounted mid-stream, so
   * there is nobody left to hand a session to.
   */
  startSessionStreamed: (request: StartSessionRequestInit) => Promise<StartSessionResponse | null>;
}

/** How each basename rule reads next to the row that broke it. */
const BASENAME_REFUSALS: Readonly<Record<AttachmentBasenameRejection, string>> = {
  empty: "An attachment needs a name.",
  separator: "must be a single name — an attachment is stored flat, with no “/” or “\\”.",
  "dot-segment": "names a directory, not a file.",
};

/** Where one row's bytes already sit on a staging host, once they have been uploaded. */
interface StagedFileLocation {
  daemonInstanceId: string;
  stagingId: string;
  fileName: string;
}

/**
 * A completed upload, remembered against the exact `File` it carried. Re-staging is decided by
 * identity rather than by name or size: a row whose file was replaced is a different upload, while a
 * renamed row is the same one (only `basename` moved, and the staged file keeps the name it was
 * uploaded under).
 */
interface StagedRow {
  file: File;
  location: StagedFileLocation;
}

/**
 * The first thing wrong with the attachment set, or `null` when the daemon would accept it. Mirrors
 * the host's own rules so the offending row is named here, rather than the whole creation failing
 * with a message that names neither row.
 */
function describeAttachmentProblem(attachments: PendingAttachment[]): string | null {
  for (const attachment of attachments) {
    const validation = validateAttachmentBasename(attachment.basename);
    if (validation.ok) continue;
    return validation.reason === "empty"
      ? BASENAME_REFUSALS.empty
      : `“${attachment.basename}” ${BASENAME_REFUSALS[validation.reason]}`;
  }
  const duplicates = duplicateBasenames(attachments.map((a) => a.basename));
  if (duplicates.length > 0) {
    const named = duplicates.map((basename) => `“${basename}”`).join(", ");
    return `Two attachments would be stored as ${named} — rename one before creating the session.`;
  }
  return null;
}

/**
 * The wire form of one attach row. A local file has become a staged file on the host that holds it; a
 * host document already names the host that owns it. `basename` is independent of either locator,
 * which is what makes a rename possible without touching the stored bytes.
 */
function toRequestAttachment(
  attachment: PendingAttachment,
  staged: StagedFileLocation | undefined,
): SessionAttachmentInit {
  if (attachment.source.case === "hostDocument") {
    return {
      basename: attachment.basename,
      source: { case: "hostDocument", value: attachment.source.document },
    };
  }
  if (staged === undefined) {
    // Unreachable: every file row is either already staged or staged by the call that builds this.
    // Raised rather than sent as an empty ref, which the host would refuse as a missing batch.
    throw new Error(`“${attachment.basename}” was not uploaded, so it cannot be attached`);
  }
  return {
    basename: attachment.basename,
    source: { case: "staged", value: staged },
  };
}

/**
 * Why one document is too large to attach. "the hosts involved", not "this host": the limit is the
 * smaller of what the staging host and the session host advertise, and those are two different hosts
 * whenever the session runs elsewhere. Naming one of them would name the wrong cap.
 */
function describeOverCap(name: string, sizeBytes: number, maxAttachmentBytes: number): string {
  return (
    `“${name}” is ${formatAttachmentBytes(sizeBytes)}, over the ` +
    `${formatAttachmentBytes(maxAttachmentBytes)} limit the hosts involved accept.`
  );
}

/** A rounded percentage, and `0` while the producer has not reported a total yet. */
function percentDone(bytesDone: number, bytesTotal: number): number {
  if (bytesTotal <= 0) return 0;
  return Math.min(100, Math.round((bytesDone * 100) / bytesTotal));
}

export function useSessionAttachments({
  client,
  sessionToken,
  sessionDaemonInstanceId,
  initialAttachments = [],
}: UseSessionAttachmentsArgs): SessionAttachments {
  const daemons = useDaemons();
  const { selectedInstanceId } = useSelectedDaemon();
  const { stageFiles } = useStagedAttachmentUpload(client, sessionToken);

  // Documents to materialize into the session before its agent starts. Nothing is uploaded until
  // Create is pressed, so an abandoned form leaves no staged bytes behind.
  const [attachments, setAttachments] = useState<PendingAttachment[]>(() =>
    initialAttachments.map((attachment) => ({ ...attachment, id: randomUuid() })),
  );
  const [pickRefusal, setPickRefusal] = useState<string | null>(null);
  const [progress, setProgress] = useState<Record<string, AttachmentProgress>>({});
  const [hostDocPickerOpen, setHostDocPickerOpen] = useState(false);

  // What has already reached a staging host, by row id. A submit that fails — a branch conflict, most
  // often, which the operator answers by re-running the creation — must not upload the same bytes
  // again: over the LiveKit data channel that is minutes per attachment, and the abandoned batch
  // stays on the host until its temp root is cleared.
  const stagedRows = useRef(new Map<string, StagedRow>());

  // Stops a start-session stream from touching this form after it unmounts. Reset on mount so a
  // remount (React strict-mode double-invoke, or a re-opened pane) starts live again.
  const cancelledRef = useRef(false);
  useEffect(() => {
    cancelledRef.current = false;
    return () => {
      cancelledRef.current = true;
    };
  }, []);

  // Local files are staged on the daemon this form's client is connected to, which is not necessarily
  // the host that will run the session. The staged ref therefore names the *staging* host, and the
  // session host fetches the bytes from there.
  const stagingDaemonInstanceId = selectedInstanceId ?? "";

  // Both hosts bound an attachment — the staging host stores the bytes and the session host
  // re-checks their size while fetching them — so the smaller advertised cap is the real limit.
  // Undefined when neither advertises one (an older daemon): the host then enforces its own.
  const advertisedCaps = [stagingDaemonInstanceId, sessionDaemonInstanceId]
    .map((instanceId) => daemons.find((d) => d.instanceId === instanceId)?.maxAttachmentBytes)
    .filter((cap): cap is number => cap !== undefined);
  const maxAttachmentBytes = advertisedCaps.length > 0 ? Math.min(...advertisedCaps) : undefined;

  const attachFiles = (files: File[]) => {
    const accepted: PendingAttachment[] = [];
    const refused: string[] = [];
    for (const file of files) {
      if (maxAttachmentBytes !== undefined && file.size > maxAttachmentBytes) {
        refused.push(describeOverCap(file.name, file.size, maxAttachmentBytes));
        continue;
      }
      accepted.push({
        id: randomUuid(),
        basename: file.name,
        sizeBytes: file.size,
        source: { case: "file", file },
      });
    }
    setPickRefusal(refused.length > 0 ? refused.join(" ") : null);
    if (accepted.length > 0) {
      setAttachments((prev) => [...prev, ...accepted]);
    }
  };

  const attachHostDocument = (pick: HostDocumentPick) => {
    if (maxAttachmentBytes !== undefined && pick.sizeBytes > maxAttachmentBytes) {
      // Refused here rather than by the host: the size came from the host's own listing, so sending
      // an upload-free reference the session host would then refuse only costs a failed creation.
      // The picker stays open, so the operator can pick something else without reopening it.
      setPickRefusal(describeOverCap(pick.basename, pick.sizeBytes, maxAttachmentBytes));
      return;
    }
    setHostDocPickerOpen(false);
    setPickRefusal(null);
    setAttachments((prev) => [
      ...prev,
      {
        id: randomUuid(),
        basename: pick.basename,
        sizeBytes: pick.sizeBytes,
        source: { case: "hostDocument", document: pick.document },
      },
    ]);
  };

  const renameAttachment = (id: string, basename: string) => {
    setAttachments((prev) => prev.map((a) => (a.id === id ? { ...a, basename } : a)));
  };

  const removeAttachment = (id: string) => {
    setAttachments((prev) => prev.filter((a) => a.id !== id));
    stagedRows.current.delete(id);
  };

  /**
   * The upload already done for this row, or `undefined` when it still has to happen. A staged batch
   * is only reusable on the host that holds it: switching the connected daemon moves where a ref has
   * to point, so the bytes go up again rather than being referenced on a host that never received
   * them.
   */
  const reusableUpload = (attachment: PendingAttachment): StagedFileLocation | undefined => {
    if (attachment.source.case !== "file") return undefined;
    const known = stagedRows.current.get(attachment.id);
    if (known === undefined) return undefined;
    if (known.file !== attachment.source.file) return undefined;
    if (known.location.daemonInstanceId !== stagingDaemonInstanceId) return undefined;
    return known.location;
  };

  const stageAttachments = async (): Promise<SessionAttachmentInit[]> => {
    const toStage: { rowId: string; file: File; progressKey: string }[] = [];
    for (const attachment of attachments) {
      if (attachment.source.case !== "file") continue;
      if (reusableUpload(attachment) !== undefined) continue;
      // Progress is keyed by basename: that is the only identity the host's own progress events
      // carry, so both halves of a row's progress (staging here, materializing there) land on the
      // same row.
      toStage.push({
        rowId: attachment.id,
        file: attachment.source.file,
        progressKey: attachment.basename,
      });
    }

    if (toStage.length > 0) {
      const files: StagedAttachmentFile[] = toStage.map(({ progressKey, file }) => ({
        key: progressKey,
        file,
      }));
      const reportStagingProgress = ({ key, bytesDone, bytesTotal }: StagedAttachmentProgress) => {
        setProgress((prev) => ({
          ...prev,
          [key]: { percent: percentDone(bytesDone, bytesTotal), phase: "staging" },
        }));
      };
      const stagingId = await stageFiles({
        daemonInstanceId: stagingDaemonInstanceId,
        files,
        onProgress: reportStagingProgress,
      });
      for (const { rowId, file } of toStage) {
        stagedRows.current.set(rowId, {
          file,
          location: {
            daemonInstanceId: stagingDaemonInstanceId,
            stagingId,
            fileName: file.name,
          },
        });
      }
    }

    return attachments.map((attachment) =>
      toRequestAttachment(attachment, reusableUpload(attachment)),
    );
  };

  /**
   * Consumed with a `cancelled` flag rather than an `AbortSignal`: the LiveKit transport accepts a
   * signal for server-streaming calls and never reads it, so aborting would look correct here and be
   * a no-op in production. A stream that ends without a result, or errors, is a failed creation.
   */
  const startSessionStreamed = async (
    request: StartSessionRequestInit,
  ): Promise<StartSessionResponse | null> => {
    let result: StartSessionResponse | null = null;
    for await (const event of client.streamStartSession(request)) {
      if (cancelledRef.current) return null;
      if (event.event.case === "attachmentProgress") {
        const reported = event.event.value;
        setProgress((prev) => ({
          ...prev,
          [reported.basename]: {
            percent: percentDone(Number(reported.bytesDone), Number(reported.bytesTotal)),
            phase: "materializing",
          },
        }));
      } else if (event.event.case === "result") {
        result = event.event.value;
      }
    }
    if (result === null) {
      throw new Error("the host ended the start-session stream without a result");
    }
    return result;
  };

  return {
    attachments,
    progress,
    stagingDaemonInstanceId,
    problem: describeAttachmentProblem(attachments),
    pickRefusal,
    hostDocPickerOpen,
    attachFiles,
    attachHostDocument,
    renameAttachment,
    removeAttachment,
    openHostDocPicker: () => setHostDocPickerOpen(true),
    closeHostDocPicker: () => setHostDocPickerOpen(false),
    resetProgress: () => setProgress({}),
    stageAttachments,
    startSessionStreamed,
  };
}
