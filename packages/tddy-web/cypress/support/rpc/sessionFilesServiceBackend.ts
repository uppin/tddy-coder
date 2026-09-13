/**
 * In-memory `session_files.SessionFilesService` backend — the files already uploaded to a session,
 * and their removal.
 *
 * Built when the thirteen file RPCs left `connection.ConnectionService` for
 * `session_files.SessionFilesService`: a fake is registered per service, so a screen's backend
 * composes the two rather than one `.implement` covering both. `aSessionFilesServiceFake` is the
 * composable half (`handlers` spread into a caller's own `.implement(SessionFilesService, …)`),
 * `aSessionFilesServiceBackend` the standalone one, mirroring `hostServiceBackend.ts`.
 *
 * Stateful on purpose: `DeleteSessionUpload` removes the matching entry, so a delete followed by a
 * reload drops the row — a fake, not a mock. The chunked-upload half of the service
 * (`UploadSessionFileChunk`, `UploadStagedAttachmentChunk`) is deliberately absent: every spec that
 * exercises it asserts on the *recorded chunks* (`backend.callsTo(…)`), which needs the method
 * registered on the spec's own backend rather than behind a shared handler.
 */

import { create } from "@bufbuild/protobuf";
import type { ServiceImpl } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  DeleteSessionUploadResponseSchema,
  ListSessionUploadsResponseSchema,
  SessionFilesService,
  SessionUploadEntrySchema,
} from "../../../src/gen/session_files_pb";

/** One file already uploaded to a session, as `ListSessionUploads` reports it. */
export interface SessionUploadInput {
  uploadId: string;
  fileName: string;
  hostPath: string;
  sizeBytes: bigint;
  uploadedAtMs: bigint;
}

export interface SessionFilesServiceScenario {
  /** The session's uploaded files. Defaults to none — the empty-state the Files tab renders. */
  uploads?: SessionUploadInput[];
}

export interface SessionFilesServiceControls {
  /** Every `{ uploadId, fileName }` passed to `DeleteSessionUpload`, in call order. */
  readonly deletedUploads: { uploadId: string; fileName: string }[];
}

/** The session-files fake as handlers, so a screen's own backend can serve it too. */
export interface SessionFilesServiceFake extends SessionFilesServiceControls {
  handlers: Partial<ServiceImpl<typeof SessionFilesService>>;
}

/**
 * Build the `session_files.SessionFilesService` handlers for `scenario`, plus the recorders a spec
 * asserts on.
 */
export function aSessionFilesServiceFake(
  scenario: SessionFilesServiceScenario = {},
): SessionFilesServiceFake {
  let uploads = [...(scenario.uploads ?? [])];
  const deletedUploads: { uploadId: string; fileName: string }[] = [];

  const handlers: Partial<ServiceImpl<typeof SessionFilesService>> = {
    listSessionUploads: async () =>
      create(ListSessionUploadsResponseSchema, {
        uploads: uploads.map((u) => create(SessionUploadEntrySchema, u)),
      }),
    deleteSessionUpload: async (req) => {
      deletedUploads.push({ uploadId: req.uploadId, fileName: req.fileName });
      uploads = uploads.filter(
        (u) => !(u.uploadId === req.uploadId && u.fileName === req.fileName),
      );
      return create(DeleteSessionUploadResponseSchema, {});
    },
  };

  return { handlers, deletedUploads };
}

export interface SessionFilesServiceBackend extends SessionFilesServiceControls {
  backend: InMemoryRpcBackend;
}

/** The same fake as a backend of its own, for a spec that needs nothing but the session-files service. */
export function aSessionFilesServiceBackend(
  scenario: SessionFilesServiceScenario = {},
): SessionFilesServiceBackend {
  const { handlers, ...controls } = aSessionFilesServiceFake(scenario);
  return {
    backend: anInMemoryRpcBackend().implement(SessionFilesService, handlers),
    ...controls,
  };
}
