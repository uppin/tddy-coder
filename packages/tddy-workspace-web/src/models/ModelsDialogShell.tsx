import { useEffect, type ReactNode } from "react";

/**
 * The modal chrome the Models & Agents dialogs share: a backdrop that dismisses on click, Escape to
 * dismiss, and `role="dialog"` + `aria-modal` on the panel itself.
 *
 * Same shape as the chrome `SessionWorkflowFilesModal` and `AgentActivityDetailDialog` established
 * (`fixed inset-0 z-50`, backdrop `onMouseDown`, document-level Escape listener) — a dialog that can
 * only be left through its own Cancel button is the odd one out on this screen, and a screen reader
 * told nothing of the modality reads the table behind it as still available.
 *
 * TODO: focus is not trapped here. No dialog in tddy-web traps it, and a trap only some dialogs have
 * is a worse contract than none — so it belongs in the shared modal chrome for every screen at once,
 * not in this one.
 */
export function ModelsDialogShell({
  testId,
  label,
  className,
  onClose,
  children,
}: {
  testId: string;
  /** The dialog's accessible name. */
  label: string;
  /** Panel sizing — each dialog owns its own dimensions. */
  className: string;
  onClose: () => void;
  children: ReactNode;
}) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-background/80 p-4"
      role="presentation"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        data-testid={testId}
        role="dialog"
        aria-modal="true"
        aria-label={label}
        className={className}
        onMouseDown={(e) => e.stopPropagation()}
      >
        {children}
      </div>
    </div>
  );
}
