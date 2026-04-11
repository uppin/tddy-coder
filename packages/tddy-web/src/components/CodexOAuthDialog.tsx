import React from "react";

import { Button } from "@/components/ui/button";

export type CodexOAuthDialogProps = {
  authorizeUrl: string | null;
  open: boolean;
  onDismiss: () => void;
  /** When the IdP blocks framing (X-Frame-Options / CSP), use non-iframe UX per design. */
  embeddingBlocked?: boolean;
};

/**
 * Modal for Codex OAuth authorize step in tddy-web.
 * Uses an iframe when embedding is allowed; otherwise shows a documented top-level-window fallback
 * (link with `target="_blank"` + `rel="noopener noreferrer"`).
 */
export function CodexOAuthDialog({
  authorizeUrl,
  open,
  onDismiss,
  embeddingBlocked = false,
}: CodexOAuthDialogProps) {
  if (!open) {
    return null;
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"
      data-testid="codex-oauth-dialog"
      role="dialog"
      aria-modal="true"
      aria-labelledby="codex-oauth-title"
    >
      <div className="bg-card text-card-foreground border-border flex max-h-[90vh] w-full max-w-lg flex-col overflow-hidden rounded-xl border shadow-lg">
        <div className="border-border flex items-center justify-between border-b px-4 py-3">
          <h2 id="codex-oauth-title" className="text-sm font-semibold">
            Codex sign-in
          </h2>
          <Button
            type="button"
            variant="outline"
            size="sm"
            data-testid="codex-oauth-dismiss"
            onClick={onDismiss}
          >
            Dismiss
          </Button>
        </div>

        <div className="min-h-[240px] flex-1 overflow-auto p-4">
          {embeddingBlocked ? (
            <div
              className="bg-muted/40 text-muted-foreground flex flex-col gap-3 rounded-lg p-4 text-sm"
              data-testid="codex-oauth-embedding-fallback"
            >
              <p>
                This provider blocks embedding the login page in the dashboard (X-Frame-Options /
                CSP). Open the authorize URL in a separate secure window to continue.
              </p>
              {authorizeUrl ? (
                <a
                  className="text-primary font-medium underline"
                  href={authorizeUrl}
                  target="_blank"
                  rel="noopener noreferrer"
                >
                  Open authorization in new window
                </a>
              ) : null}
            </div>
          ) : authorizeUrl ? (
            <iframe
              title="Codex OAuth authorize"
              className="h-[min(60vh,420px)] w-full rounded-md border border-border"
              src={authorizeUrl}
              sandbox="allow-forms allow-scripts allow-same-origin allow-popups"
            />
          ) : (
            <p className="text-muted-foreground text-sm">Waiting for authorize URL…</p>
          )}
        </div>
      </div>
    </div>
  );
}
