import { useEffect, useRef } from "react";
import type { VideoTrack } from "livekit-client";

export interface ParticipantVideoPreviewDialogProps {
  /** LiveKit identity for labeling. */
  identity: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** When set, attached to the preview &lt;video&gt; while open; detached on close. */
  videoTrack?: VideoTrack | null;
}

/**
 * Modal for viewing a participant's camera from the shared Room connection (livekit-client attach/detach).
 * Dismiss via close control, overlay click, or Escape — consistent with other tddy-web modals.
 */
export function ParticipantVideoPreviewDialog({
  identity,
  open,
  onOpenChange,
  videoTrack,
}: ParticipantVideoPreviewDialogProps) {
  const videoRef = useRef<HTMLVideoElement>(null);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        console.info("[tddy-web:participant-video] ParticipantVideoPreviewDialog: Escape dismiss", {
          identity,
        });
        onOpenChange(false);
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open, onOpenChange, identity]);

  useEffect(() => {
    const el = videoRef.current;
    if (!open || !videoTrack || !el) {
      return;
    }
    console.debug("[tddy-web:participant-video] ParticipantVideoPreviewDialog: attach track", {
      identity,
    });
    videoTrack.attach(el);
    return () => {
      console.debug("[tddy-web:participant-video] ParticipantVideoPreviewDialog: detach track", {
        identity,
      });
      videoTrack.detach(el);
    };
  }, [open, videoTrack, identity]);

  useEffect(() => {
    if (open) {
      console.info("[tddy-web:participant-video] ParticipantVideoPreviewDialog: opened", {
        identity,
        hasTrack: videoTrack != null,
      });
    }
  }, [open, identity, videoTrack]);

  if (!open) {
    return null;
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"
      role="presentation"
      data-testid="participant-video-dialog-overlay"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) {
          console.info("[tddy-web:participant-video] ParticipantVideoPreviewDialog: overlay dismiss", {
            identity,
          });
          onOpenChange(false);
        }
      }}
    >
      <div
        data-testid="participant-video-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="participant-video-dialog-title"
        className="flex w-full max-w-2xl flex-col overflow-hidden rounded-lg border border-border bg-background shadow-lg"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <header className="flex shrink-0 items-center justify-between border-b border-border px-4 py-3">
          <h2 id="participant-video-dialog-title" className="text-lg font-semibold">
            Video — {identity}
          </h2>
          <button
            type="button"
            data-testid="participant-video-dialog-close"
            className="rounded-md px-2 py-1 text-sm text-muted-foreground hover:bg-muted"
            onClick={() => {
              console.info("[tddy-web:participant-video] ParticipantVideoPreviewDialog: close control", {
                identity,
              });
              onOpenChange(false);
            }}
          >
            Close
          </button>
        </header>
        <div
          data-testid="participant-video-preview"
          className="min-h-[200px] bg-black/80 p-2"
        >
          {videoTrack ? (
            <video
              ref={videoRef}
              className="h-auto max-h-[70vh] w-full object-contain"
              playsInline
              muted
            />
          ) : (
            <p className="p-4 text-center text-sm text-muted-foreground">
              No camera track available for this participant.
            </p>
          )}
        </div>
      </div>
    </div>
  );
}
