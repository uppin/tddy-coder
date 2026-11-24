import { useCallback, useRef, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type {
  CloseSessionPreview,
  CloseSessionStep,
  SessionEntry,
  SessionService,
} from "tddy-client-web/gen/session_pb";
import { hostNotConnected, rpcFailureText } from "tddy-client-web";
import { requestSessionsRefresh } from "./lib/sessionsRefreshBridge";
import { closableChildren } from "./utils/closeSessionChildren";
import { owningHostForSession } from "./utils/crossHostSessions";
import { sessionDrawerLabel } from "./utils/sessionDrawerLabel";

export interface UseSessionClosesArgs {
  /** The rows the drawer lists — where a session's children are found. */
  sessions: ReadonlyArray<SessionEntry>;
  selectedInstanceId: string | null;
  /** The owning host's `SessionService` client; `null` when no wire reaches that host. */
  clientForHost: (instanceId: string) => Client<typeof SessionService> | null;
  sessionToken: string;
  /** A daemon instance id's display label — names the host a close could not reach. */
  hostLabelForInstance: (instanceId: string) => string;
}

/** A close the operator is asked to confirm: what it would lose, and the sessions it could take along. */
export interface CloseDialogRequest {
  /** Distinct per opening, so a re-opened dialog starts from its own state. */
  readonly key: number;
  readonly session: SessionEntry;
  readonly preview: CloseSessionPreview;
  /** The sessions this one started that are still active or hold a worktree. */
  readonly children: readonly SessionEntry[];
}

/** What the operator confirmed in the close dialog. */
export interface CloseConfirmation {
  /** The fingerprint of the session's own preview, as the dialog last showed it. */
  readonly fingerprint: string;
  /** The children chosen to close after it, each with the fingerprint of its own preview. */
  readonly children: ReadonlyArray<{ readonly session: SessionEntry; readonly fingerprint: string }>;
}

/** A close that did not do everything it set out to, and why. */
export interface CloseFailure {
  readonly sessionId: string;
  /** The row's drawer label, so the notice names the session the operator closed. */
  readonly label: string;
  /** Why the close could not run at all (an RPC failure); `null` when it ran and steps failed. */
  readonly reason: string | null;
  /** Every step the daemon reported failed or skipped, in order. */
  readonly failedSteps: readonly CloseSessionStep[];
}

/** How one close ended: every step done, something failed (reported), or the daemon asked again. */
type CloseRun = "closed" | "failed" | "asked-again";

function failureOf(
  session: SessionEntry,
  reason: string | null,
  failedSteps: readonly CloseSessionStep[],
): CloseFailure {
  return { sessionId: session.sessionId, label: sessionDrawerLabel(session), reason, failedSteps };
}

const CHECKING_MESSAGE = "Checking what closing would lose…";
const CLOSING_MESSAGE = "Closing…";

/**
 * The sessions drawer's closes (docs/ft/web/session-drawer.md § Closing a session): the preview a
 * row's × or trash bin asks for, the close dialog when there is anything to confirm, and each close
 * running on its own — tracked per session id, so any number run at once and none blocks the drawer.
 */
export function useSessionCloses({
  sessions,
  selectedInstanceId,
  clientForHost,
  sessionToken,
  hostLabelForInstance,
}: UseSessionClosesArgs) {
  /** Session id → what its running close is doing now (the spinner's hover text). */
  const [closing, setClosing] = useState<ReadonlyMap<string, string>>(() => new Map());
  const [failures, setFailures] = useState<readonly CloseFailure[]>([]);
  /** Dialogs waiting on the operator, shown one at a time in the order they were asked for. */
  const [dialogs, setDialogs] = useState<readonly CloseDialogRequest[]>([]);
  const nextDialogKey = useRef(0);

  const markClosing = useCallback((sessionId: string, message: string) => {
    setClosing((prev) => new Map(prev).set(sessionId, message));
  }, []);

  const unmarkClosing = useCallback((sessionId: string) => {
    setClosing((prev) => {
      if (!prev.has(sessionId)) return prev;
      const next = new Map(prev);
      next.delete(sessionId);
      return next;
    });
  }, []);

  const report = useCallback((failure: CloseFailure) => {
    setFailures((prev) => [...prev.filter((f) => f.sessionId !== failure.sessionId), failure]);
  }, []);

  const clearFailure = useCallback((sessionId: string) => {
    setFailures((prev) =>
      prev.some((f) => f.sessionId === sessionId) ? prev.filter((f) => f.sessionId !== sessionId) : prev,
    );
  }, []);

  const openDialog = useCallback(
    (session: SessionEntry, preview: CloseSessionPreview, children: readonly SessionEntry[]) => {
      nextDialogKey.current += 1;
      const request: CloseDialogRequest = { key: nextDialogKey.current, session, preview, children };
      setDialogs((prev) => [...prev.filter((d) => d.session.sessionId !== session.sessionId), request]);
    },
    [],
  );

  // A session is closed on the daemon that owns it — never on the selected one in its place.
  const clientFor = useCallback(
    (session: SessionEntry): Client<typeof SessionService> => {
      const owner = owningHostForSession(session, selectedInstanceId ?? "");
      const client = clientForHost(owner);
      if (!client) throw hostNotConnected(hostLabelForInstance(owner));
      return client;
    },
    [selectedInstanceId, clientForHost, hostLabelForInstance],
  );

  /** What closing `session` would lose, asked of its owning host. Rejects with the RPC failure. */
  const previewClose = useCallback(
    async (session: SessionEntry): Promise<CloseSessionPreview> =>
      clientFor(session).previewCloseSession({ sessionToken, sessionId: session.sessionId }),
    [clientFor, sessionToken],
  );

  /** Run one close to its end, showing its progress on the row and reporting what failed. */
  const runClose = useCallback(
    async (
      session: SessionEntry,
      children: readonly SessionEntry[],
      confirmed: boolean,
      confirmedFingerprint: string,
    ): Promise<CloseRun> => {
      const sessionId = session.sessionId;
      markClosing(sessionId, CLOSING_MESSAGE);
      clearFailure(sessionId);
      try {
        const events = clientFor(session).closeSession({
          sessionToken,
          sessionId,
          confirmed,
          confirmedFingerprint,
        });
        for await (const { event } of events) {
          if (event.case === "step") {
            markClosing(sessionId, event.value.message || event.value.subject);
          } else if (event.case === "needsConfirmation") {
            // More is at risk than the operator confirmed; the daemon did nothing further.
            openDialog(session, event.value, children);
            return "asked-again";
          } else if (event.case === "outcome") {
            const { ok, failedSteps } = event.value;
            if (!ok || failedSteps.length > 0) {
              report(failureOf(session, failedSteps.length > 0 ? null : "the close did not complete", failedSteps));
              return "failed";
            }
            return "closed";
          }
        }
        report(failureOf(session, "the host ended the close without reporting its outcome", []));
        return "failed";
      } catch (err) {
        report(failureOf(session, rpcFailureText(err), []));
        return "failed";
      } finally {
        unmarkClosing(sessionId);
        requestSessionsRefresh();
      }
    },
    [clientFor, sessionToken, markClosing, unmarkClosing, clearFailure, openDialog, report],
  );

  /**
   * The row's × or trash bin: ask what the close would lose, then close at once when nothing needs
   * confirming and the session started no closable child, or open the dialog otherwise.
   */
  const requestClose = useCallback(
    (session: SessionEntry) => {
      const sessionId = session.sessionId;
      if (closing.has(sessionId)) return;
      const children = closableChildren(session, sessions);
      markClosing(sessionId, CHECKING_MESSAGE);
      clearFailure(sessionId);
      void (async () => {
        let preview: CloseSessionPreview;
        try {
          preview = await previewClose(session);
        } catch (err) {
          unmarkClosing(sessionId);
          report(failureOf(session, rpcFailureText(err), []));
          return;
        }
        if (!preview.needsConfirmation && children.length === 0) {
          await runClose(session, children, false, "");
          return;
        }
        unmarkClosing(sessionId);
        openDialog(session, preview, children);
      })();
    },
    [closing, sessions, markClosing, unmarkClosing, clearFailure, previewClose, report, runClose, openDialog],
  );

  const cancelDialog = useCallback((request: CloseDialogRequest) => {
    setDialogs((prev) => prev.filter((d) => d.key !== request.key));
  }, []);

  /** Close the session with what the operator saw, then each child chosen, each with its own list. */
  const confirmDialog = useCallback(
    (request: CloseDialogRequest, confirmation: CloseConfirmation) => {
      setDialogs((prev) => prev.filter((d) => d.key !== request.key));
      void (async () => {
        const run = await runClose(request.session, request.children, true, confirmation.fingerprint);
        // Children are closed only after the session itself closed. Asked again: the re-opened
        // dialog offers the children choice afresh. Failed: its failure is shown, and the operator
        // retries the session, then its children.
        if (run !== "closed") return;
        await Promise.all(
          confirmation.children.map(({ session, fingerprint }) =>
            runClose(session, closableChildren(session, sessions), true, fingerprint),
          ),
        );
      })();
    },
    [runClose, sessions],
  );

  return {
    closing,
    failures,
    dialog: dialogs[0] ?? null,
    requestClose,
    previewClose,
    cancelDialog,
    confirmDialog,
  };
}
