import { Paperclip } from "lucide-react";
import type { WorkItemFile } from "./workQueue";

/** Bytes as the row states them — small counts exactly, larger ones rounded once. */
export function formatAttachmentBytes(sizeBytes: number): string {
  if (sizeBytes < 1024) return `${sizeBytes} B`;
  const kb = sizeBytes / 1024;
  if (kb < 1024) return `${Math.round(kb)} KB`;
  return `${(kb / 1024).toFixed(1)} MB`;
}

/**
 * The attachment count on a row's summary header: a paperclip and the number of files the item
 * carries. Rendered only for an item that has files — an item without any states nothing.
 */
export function WorkItemFilesBadge({
  itemId,
  files,
}: {
  itemId: string;
  files: WorkItemFile[];
}) {
  if (files.length === 0) return null;
  const names = files.map((file) => file.basename).join(", ");
  return (
    <span
      data-testid={`work-item-attachments-${itemId}`}
      title={names}
      className="flex items-center gap-0.5 text-muted-foreground"
    >
      <Paperclip className="h-3.5 w-3.5" />
      <span className="text-xs">{files.length}</span>
    </span>
  );
}

/** The item's files, listed in the expanded row: one line each, name and size. The body is hidden
 * rather than unmounted when the row collapses, so the id names its item. */
export function WorkItemFilesList({
  itemId,
  files,
}: {
  itemId: string;
  files: WorkItemFile[];
}) {
  if (files.length === 0) return null;
  return (
    <div
      data-testid={`work-item-files-${itemId}`}
      className="flex flex-col gap-0.5 text-xs text-muted-foreground"
    >
      {files.map((file) => (
        <span
          key={file.basename}
          data-testid={`work-item-file-${file.basename}`}
          className="truncate"
          title={file.basename}
        >
          <Paperclip className="mr-1 inline h-3 w-3" />
          {file.basename} · {formatAttachmentBytes(file.sizeBytes)}
        </span>
      ))}
    </div>
  );
}
