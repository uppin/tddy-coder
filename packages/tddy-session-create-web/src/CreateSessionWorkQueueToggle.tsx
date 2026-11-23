import { useState, type ReactNode } from "react";

export interface CreateSessionWorkQueueToggleProps {
  withWorkQueue: boolean;
  setWithWorkQueue: (withWorkQueue: boolean) => void;
}

/**
 * "With work queue" (`#workqueue` 5/14): start the session with an empty work queue it owns. A queue
 * comes into being only here, at start — a session started without it has none and no Work queue tab.
 */
export function CreateSessionWorkQueueToggle({
  withWorkQueue,
  setWithWorkQueue,
}: CreateSessionWorkQueueToggleProps) {
  return (
    <div>
      <label className="flex items-center gap-2 text-sm text-muted-foreground">
        <input
          data-testid="create-session-with-work-queue"
          type="checkbox"
          className="h-4 w-4 rounded border-input"
          checked={withWorkQueue}
          onChange={(e) => setWithWorkQueue(e.target.checked)}
        />
        With work queue
      </label>
    </div>
  );
}

/** The form's "With work queue" choice and its control, unchecked until the user checks it. */
export function useWorkQueueToggle(): { withWorkQueue: boolean; workQueueToggle: ReactNode } {
  const [withWorkQueue, setWithWorkQueue] = useState(false);
  return {
    withWorkQueue,
    workQueueToggle: (
      <CreateSessionWorkQueueToggle withWorkQueue={withWorkQueue} setWithWorkQueue={setWithWorkQueue} />
    ),
  };
}
