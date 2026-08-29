/**
 * Where a `SessionContextDoc` lives relative to its session's `artifacts/` root.
 *
 * Only the server knows: a user-attached document sits at `attachments/<basename>` and a per-PR
 * document at `prs/<node_id>/<basename>`, which no derivation from `kind` + `basename` can express.
 * The kind-based derivation remains for a daemon that predates `relative_path` and therefore sends it
 * empty — that daemon also has no nested documents to describe.
 *
 * Shared by the host-document picker (which offers the doc) and the PR-stack Start-session flow
 * (which pre-attaches it): addressing the same document two ways is a silent failure, since the
 * daemon merely reports a mis-addressed document as missing.
 *
 * Feature: docs/ft/coder/pr-stack-docs.md § Listing per-PR documents
 */

import { SessionContextDocKind, type SessionContextDoc } from "../../../gen/connection_pb";

export function contextDocRelativePath(doc: SessionContextDoc): string {
  if (doc.relativePath) return doc.relativePath;
  return doc.kind === SessionContextDocKind.ATTACHMENT
    ? `attachments/${doc.basename}`
    : doc.basename;
}
