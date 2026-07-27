import React, { useRef, useState } from "react";
import type { SessionAttachment } from "../../gen/connection_pb";
import { create } from "@bufbuild/protobuf";
import {
  SessionAttachmentSchema,
  StagedAttachmentRefSchema,
} from "../../gen/connection_pb";
import { useStagedAttachmentUpload, type StagedFile } from "../../hooks/useStagedAttachmentUpload";
import { Button } from "../ui/button";

export interface StartSessionAttachmentsFieldProps {
  sessionToken: string;
  daemonInstanceId: string;
  attachments: SessionAttachment[];
  onChange: (attachments: SessionAttachment[]) => void;
  disabled?: boolean;
}

function toSessionAttachments(staged: StagedFile[]): SessionAttachment[] {
  return staged.map((file) =>
    create(SessionAttachmentSchema, {
      basename: file.basename,
      source: {
        case: "staged",
        value: create(StagedAttachmentRefSchema, {
          daemonInstanceId: file.daemonInstanceId,
          stagingId: file.stagingId,
          fileName: file.fileName,
        }),
      },
    }),
  );
}

export function StartSessionAttachmentsField({
  sessionToken,
  daemonInstanceId,
  attachments,
  onChange,
  disabled = false,
}: StartSessionAttachmentsFieldProps) {
  const fileInputRef = useRef<HTMLInputElement>(null);
  const { uploadFiles } = useStagedAttachmentUpload(sessionToken, daemonInstanceId);
  const [uploading, setUploading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handlePick = async (event: React.ChangeEvent<HTMLInputElement>) => {
    const picked = Array.from(event.target.files ?? []);
    event.target.value = "";
    if (picked.length === 0) return;

    setUploading(true);
    setError(null);
    try {
      const staged = await uploadFiles(picked);
      onChange([...attachments, ...toSessionAttachments(staged)]);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
    } finally {
      setUploading(false);
    }
  };

  const removeAt = (index: number) => {
    onChange(attachments.filter((_, i) => i !== index));
  };

  return (
    <div className="space-y-2" data-testid="create-session-attachments-field">
      <div className="flex items-center justify-between gap-2">
        <label className="text-sm text-muted-foreground">Attached documents</label>
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={disabled || uploading || !daemonInstanceId}
          onClick={() => fileInputRef.current?.click()}
          data-testid="create-session-attachments-add-btn"
        >
          {uploading ? "Uploading…" : "Add documents"}
        </Button>
      </div>
      <input
        ref={fileInputRef}
        type="file"
        multiple
        className="hidden"
        data-testid="create-session-attachments-input"
        onChange={handlePick}
      />
      {attachments.length > 0 ? (
        <ul className="space-y-1">
          {attachments.map((attachment, index) => (
            <li
              key={`${attachment.basename}-${index}`}
              className="flex items-center justify-between gap-2 rounded border border-border px-2 py-1 text-sm"
              data-testid={`create-session-attachment-row-${index}`}
            >
              <span className="truncate" title={attachment.basename}>
                {attachment.basename}
              </span>
              <Button
                type="button"
                variant="ghost"
                size="sm"
                disabled={disabled || uploading}
                onClick={() => removeAt(index)}
                data-testid={`create-session-attachment-remove-${index}`}
              >
                Remove
              </Button>
            </li>
          ))}
        </ul>
      ) : (
        <p className="text-xs text-muted-foreground">
          Optional files copied into the new session&apos;s <code>attachments/</code> folder before the agent starts.
        </p>
      )}
      {error ? (
        <p className="text-sm text-destructive" data-testid="create-session-attachments-error">
          {error}
        </p>
      ) : null}
    </div>
  );
}
