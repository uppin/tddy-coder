/**
 * A host's desktop, in the overlay tddy already has.
 *
 * A thin host-scoped mount of `ScreenSharingOverlay` — the same LiveKit `VideoTrack` subscription
 * the session-scoped tab uses. **No browser-side VNC/RDP client is added here, ever**: rendering is
 * always a daemon-produced video track, which is the existing architecture and not a limitation to
 * route around.
 *
 * ⚠ This surface cannot exist without the `media` capability. `IPC_CAPABILITIES` is `{"rpc"}` — a
 * frame pipe carries no video — so on the desktop build's own IPC-reached host a track cannot
 * arrive at all. Following `InspectorTabs`, the entry point is **removed** rather than shown broken;
 * that gate is `HostRowRemoteDesktop`'s, and this component is only ever mounted past it.
 *
 * **The overlay opens on the click, not on the reply.** Starting a bridge is a spawn on a remote
 * machine, so there is a real interval between asking and a track existing — and a request that
 * fails has to say so somewhere. Rendering nothing until `StartHostStream` returns would leave the
 * operator with an action that appeared to do nothing, and no place to put the error.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type { Client } from "@connectrpc/connect";
import { create, toBinary } from "@bufbuild/protobuf";
import { Code, ConnectError } from "@connectrpc/connect";
import { HostPromptKind, HostService } from "../../gen/host_pb";
import {
  DesktopAuthMethod,
  DesktopCredentialAnswerSchema,
  Protocol,
  ScreenSharingService,
  type ScreenSharingTarget,
  type StartStreamResponse,
} from "../../gen/screen_sharing_pb";
import { useAuthContext } from "../../hooks/authProvider";
import { useCommonRoom } from "../../hooks/useCommonRoom";
import { encryptForHost } from "../../lib/encryptForHost";
import { presenceIdentityForUser } from "../../lib/presenceIdentity";
import { screenSharingStartFailureText } from "../../lib/screenSharingStartFailure";
import { useHostClient } from "../../rpc/connections/registry";
import { useHostPrompts, type HostPromptEventLike } from "../../rpc/useHostPrompts";
import { ScreenSharingOverlay } from "../sessions/ScreenSharingOverlay";
import { HostDesktopSignInDialog } from "./HostDesktopSignInDialog";

export interface HostDesktopOverlayProps {
  /** The daemon instance whose desktop this is — host scope's whole addressing change. */
  hostId: string;
  /** The port node 7's probe found serving. Port discovery is out of scope; this is what it checked. */
  port: number;
  /** `screen_sharing.proto`'s `Protocol`, as node 7's reading reports it. */
  protocol: Protocol;
  /**
   * The X11 display to open, for an X11-display channel: sent as `AddHostTarget.x11Display` so the
   * daemon starts x11vnc for it. Absent for a desktop already listening on `port`.
   */
  x11Display?: number;
  onClose: () => void;
}

/**
 * The address a target built from a probe reading points at.
 *
 * Node 7 probes each daemon's **own loopback** (`remote_desktop_probe.rs`'s `PROBE_HOST`), so this
 * is not a guess about where the desktop is: it is the address the reachable/unreachable answer was
 * made against. A desktop on some other machine's loopback is that daemon's reading to take.
 */
const PROBED_HOST = "127.0.0.1";

/** What a target created from a probe reading is called, so an operator can tell it apart later. */
const PROBED_TARGET_LABEL = "Desktop";

/** Whether `target` is the one a connect on this channel reuses: same display, or same endpoint. */
function isTargetOfChannel(
  target: ScreenSharingTarget,
  port: number,
  protocol: Protocol,
  x11Display: number | undefined,
): boolean {
  if (x11Display !== undefined) {
    return target.x11Display === x11Display && target.protocol === protocol;
  }
  return (
    target.x11Display === undefined &&
    target.host === PROBED_HOST &&
    target.port === port &&
    target.protocol === protocol
  );
}

/** What the overlay says when the desktop advertises nothing tddy can sign in with. */
function noSupportedSignInText(unsupportedVncTypes: number[]): string {
  return unsupportedVncTypes.length === 0
    ? "This desktop offers no sign-in tddy supports."
    : `This desktop offers no sign-in tddy supports: it advertises VNC security types ${unsupportedVncTypes.join(", ")}.`;
}

/**
 * The host-scoped target for the channel being opened, creating one the first time.
 *
 * A row has a channel; `StartHostStream` takes a target id. Matching on the channel rather than
 * adding unconditionally is what stops one target accumulating per connect. An X11 display is the
 * channel `(protocol, x11Display)` names; any other channel is the listening endpoint
 * `(host, port, protocol)`, and never a display's target.
 *
 * `sessionToken` is threaded in rather than left to the transport. The daemon gates every
 * host-scoped call on it, and the auth gate every LiveKit-reached host is wrapped in
 * (`rpc/authGatedTransport.ts`) rewrites the field **only where the request already carries one** —
 * so a request that omits it is not quietly repaired on the way out, it is rejected on arrival.
 */
async function targetForChannel(
  client: Client<typeof ScreenSharingService>,
  sessionToken: string,
  hostId: string,
  port: number,
  protocol: Protocol,
  x11Display: number | undefined,
): Promise<string> {
  const existing = await client.listHostTargets({ sessionToken, daemonInstanceId: hostId });
  const match = existing.targets.find((target) =>
    isTargetOfChannel(target, port, protocol, x11Display),
  );
  if (match) return match.id;

  const added = await client.addHostTarget({
    sessionToken,
    daemonInstanceId: hostId,
    label: x11Display === undefined ? PROBED_TARGET_LABEL : `Display :${x11Display}`,
    host: PROBED_HOST,
    port,
    protocol,
    username: "",
    x11Display,
  });
  return added.targetId;
}

/** What `signInAndStart` needs from the overlay that is opening a desktop. */
interface SignInAndStartInputs {
  client: Client<typeof ScreenSharingService>;
  hostId: string;
  targetId: string;
  /** The session the calls are gated on, read at call time — see `sessionTokenRef`. */
  sessionToken: () => string;
  /** Whether the overlay has closed since the start began, so nothing more is set on it. */
  isCancelled: () => boolean;
  /** How many answers the overlay has sent, so a start can tell whether it was refused one. */
  answersSent: { readonly current: number };
  setOffered: (methods: DesktopAuthMethod[]) => void;
  setStream: (stream: StartStreamResponse) => void;
  setError: (error: string) => void;
  setSignInError: (error: string) => void;
  setQuestion: (question: null) => void;
}

/**
 * Inspect the desktop, then start its stream — asking again for as long as the desktop refuses a
 * sign-in the operator has just given.
 *
 * Resolves once there is a stream, once the desktop is known to offer nothing tddy supports, or
 * once the overlay has closed; rejects with the failure the overlay is to show.
 */
async function signInAndStart(inputs: SignInAndStartInputs): Promise<void> {
  const { client, hostId, targetId, sessionToken, isCancelled, setOffered, setError } = inputs;
  // Ask what the desktop advertises before starting anything: the form is shaped by it, and a
  // desktop with nothing tddy supports must not start a bridge that can only fail.
  const inspection = await client.inspectHostDesktop({
    sessionToken: sessionToken(),
    daemonInstanceId: hostId,
    targetId,
  });
  if (isCancelled()) return;
  const methods = inspection.offered.filter((method) => method !== DesktopAuthMethod.UNSPECIFIED);
  if (methods.length === 0) {
    setError(noSupportedSignInText(inspection.unsupportedVncTypes));
    return;
  }
  setOffered(methods);
  await startUntilSignedIn(inputs, methods.includes(DesktopAuthMethod.NONE));
}

/**
 * Start the desktop's stream, starting it again each time the desktop refuses a sign-in the
 * operator has just given — unless it needed no sign-in at all.
 */
async function startUntilSignedIn(
  inputs: SignInAndStartInputs,
  needsNoSignIn: boolean,
): Promise<void> {
  const { client, hostId, targetId, sessionToken, isCancelled, answersSent, ...set } = inputs;
  // Each start blocks while the host asks for the desktop's credentials — the question arrives
  // below, on the prompt feed, and the answer releases the start awaited here. A desktop that
  // refuses the credentials fails the start with its reason, and the host is started again so it
  // asks again; a desktop that needed none has nothing to ask again about.
  for (;;) {
    const answersBefore = answersSent.current;
    try {
      const started = await client.startHostStream({
        sessionToken: sessionToken(),
        daemonInstanceId: hostId,
        targetId,
      });
      if (isCancelled()) return;
      set.setStream(started);
      return;
    } catch (e) {
      if (isCancelled()) return;
      // Asked again only when this start was refused a sign-in the operator just gave: a
      // refusal that no answer preceded (a remembered credential the desktop turned down) has
      // nothing to ask about yet, and starting again at once would only spin on it.
      const refusedAnAnswer = answersSent.current > answersBefore;
      if (
        needsNoSignIn ||
        !refusedAnAnswer ||
        !(e instanceof ConnectError) ||
        e.code !== Code.Unauthenticated
      ) {
        throw e;
      }
      set.setSignInError(screenSharingStartFailureText(e));
      set.setQuestion(null);
    }
  }
}

export function HostDesktopOverlay({
  hostId,
  port,
  protocol,
  x11Display,
  onClose,
}: HostDesktopOverlayProps) {
  const client = useHostClient(ScreenSharingService, hostId);
  const hostService = useHostClient(HostService, hostId);
  const { user, sessionToken } = useAuthContext();
  const [stream, setStream] = useState<StartStreamResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  // The sign-in methods the desktop advertised and tddy supports, once inspected.
  const [offered, setOffered] = useState<DesktopAuthMethod[] | null>(null);
  // The desktop's reason for refusing the last sign-in, shown when the host asks again.
  const [signInError, setSignInError] = useState<string | undefined>(undefined);
  // The question already answered: one prompt takes one answer, and the feed holds it until the
  // next one replaces it.
  const answeredPromptId = useRef<string | null>(null);
  // How many answers this overlay has sent, so a start can tell whether it was refused one.
  const answersSent = useRef(0);
  // The question being answered; latched — see the effect that sets it.
  const [question, setQuestion] = useState<HostPromptEventLike | null>(null);

  // The session every host-scoped call is gated on, read at call time rather than closed over.
  // Putting it in the effect's dependencies instead would tear the bridge down and start a second
  // one every time the five-minute access token is re-minted, mid-desktop.
  const sessionTokenRef = useRef(sessionToken);
  sessionTokenRef.current = sessionToken;

  // Start on mount and stop on unmount: the bridge process lives exactly as long as this overlay is
  // open, so closing it releases the process rather than leaving one per desktop an operator looked
  // at. The stop is addressed at the target the start actually used, which is why it is captured
  // here rather than re-resolved.
  useEffect(() => {
    if (!client) return;
    let cancelled = false;
    let startedTargetId: string | null = null;

    void (async () => {
      try {
        const targetId = await targetForChannel(
          client,
          sessionTokenRef.current ?? "",
          hostId,
          port,
          protocol,
          x11Display,
        );
        if (cancelled) return;
        startedTargetId = targetId;
        await signInAndStart({
          client,
          hostId,
          targetId,
          sessionToken: () => sessionTokenRef.current ?? "",
          isCancelled: () => cancelled,
          answersSent,
          setOffered,
          setStream,
          setError,
          setSignInError,
          setQuestion,
        });
      } catch (e) {
        if (cancelled) return;
        // The start failed, and the overlay says why in place of "Connecting…": a refused sign-in
        // names the desktop's reason, and nothing here waits for a stream that will not come.
        setError(screenSharingStartFailureText(e));
      }
    })();

    return () => {
      cancelled = true;
      if (startedTargetId) {
        void client
          .stopHostStream({
            sessionToken: sessionTokenRef.current ?? "",
            daemonInstanceId: hostId,
            targetId: startedTargetId,
          })
          .catch(() => {
            // The overlay is already gone; a failed stop is the daemon's to reconcile, and there is
            // no surface left to report it on.
          });
      }
    };
  }, [client, hostId, port, protocol, x11Display]);

  // The bridge publishes into the room the start reply names, which is not this page's common room:
  // joining it is the same mint-and-connect `useCommonRoom` already performs, with no coordinates
  // until there is a reply.
  const identity = useMemo(
    () => (user ? presenceIdentityForUser(user.login) : undefined),
    [user],
  );
  const { room } = useCommonRoom(stream?.livekitUrl, stream?.livekitRoom, identity);

  // Read only while our own start is blocked: once there is a stream or a failure, this host has
  // nothing left to ask about this desktop, and a screenful of hosts nobody is opening a desktop on
  // opens no streams at all.
  const awaitingStart = stream === null && error === null;
  const raised = useHostPrompts(awaitingStart ? hostId : null);

  // The question this dialog is answering, **latched** rather than read off the live feed.
  //
  // `useHostPrompts` holds a single slot and overwrites it, and the feed is per host, not per
  // surface: an add-key passphrase prompt raised on this same host while the operator is typing a
  // desktop password would replace the value a derived `question` reads from. Derived, the dialog
  // would then vanish mid-answer — the typing lost, and the host's `StartHostStream` left blocked
  // until the prompt expires with nothing left on screen to answer it. Held, an unrelated prompt
  // passes by without touching the question that is being answered.
  //
  // Only this overlay's own kind is ever latched. The same feed carries the key passphrase
  // `HostAddKeyAction` raises, and answering that one with a desktop password would send a secret
  // to a question nobody here asked.
  useEffect(() => {
    if (!awaitingStart) {
      // The start settled — opened or failed — so whatever it was blocked on is no longer being
      // waited for, and a dialog left standing over a desktop asks about nothing.
      setQuestion(null);
      return;
    }
    if (raised === null || raised.kind !== HostPromptKind.DESKTOP_CREDENTIALS) return;
    // Answered already: the feed still holds it until the host's next question replaces it.
    if (raised.promptId === answeredPromptId.current) return;
    // First one wins: the question the operator is answering is not replaced under them.
    setQuestion((held) => held ?? raised);
  }, [awaitingStart, raised]);

  /** Send the ciphertext of the answer. The credential itself never reaches this component. */
  const answerWithCredentials = async (asked: HostPromptEventLike, encryptedAnswer: Uint8Array) => {
    if (!hostService) return;
    answeredPromptId.current = asked.promptId;
    answersSent.current += 1;
    try {
      const reply = await hostService.answerHostPrompt({
        sessionToken: sessionToken ?? "",
        daemonInstanceId: hostId,
        promptId: asked.promptId,
        encryptedAnswer,
      });
      // A refused answer leaves the host waiting on a question it will never get another answer to,
      // so say so now rather than leaving the operator in front of a dialog until the prompt expires.
      if (!reply.accepted) {
        setError(`That sign-in was not accepted: ${reply.rejectionReason}`);
      }
    } catch (e) {
      setError(`The sign-in could not be sent: ${messageOf(e)}`);
    }
  };

  // A desktop that needs no sign-in is still asked its question on every start, because the host
  // cannot know in advance which desktops want one. There is nothing for an operator to type, so the
  // overlay answers it itself — with the "no authentication" answer, which carries no secret and so
  // needs no key-continuity check.
  const needsNoSignIn = offered?.includes(DesktopAuthMethod.NONE) ?? false;
  useEffect(() => {
    if (question === null || !needsNoSignIn || answeredPromptId.current === question.promptId) {
      return;
    }
    answeredPromptId.current = question.promptId;
    void (async () => {
      try {
        const none = toBinary(
          DesktopCredentialAnswerSchema,
          create(DesktopCredentialAnswerSchema, { method: DesktopAuthMethod.NONE }),
        );
        await answerWithCredentials(question, await encryptForHost(question.hostPublicKey, none));
      } catch (e) {
        setError(`The sign-in could not be sent: ${messageOf(e)}`);
      }
    })();
    // `answerWithCredentials` is re-created every render; the answered-prompt guard is what makes
    // this run once per question.
  }, [question, needsNoSignIn]);

  // Portalled to `document.body`, not rendered where it was mounted from. The row section this
  // overlay is opened from is a `<span>` (`HostRowTooling`), and a `<div>` is not permitted inside
  // one: an HTML parser hoists it out on any SSR or hydration path, and even client-side a block
  // element dropped into the row's inline flex flow shifts the row for as long as the overlay is
  // open. `ScreenSharingOverlay` and the connecting state are both `fixed inset-0 z-50` — neither
  // was ever in flow — so the body is where they already behave as though they are.
  return createPortal(
    <div data-testid={`host-desktop-overlay-${hostId}`}>
      {question !== null && offered !== null && !needsNoSignIn && (
        <HostDesktopSignInDialog
          question={question}
          offered={offered}
          error={signInError}
          onSubmit={(encryptedAnswer) => void answerWithCredentials(question, encryptedAnswer)}
          // Declining the question is declining the desktop: the start has nothing to proceed with,
          // and closing releases it rather than leaving an overlay in front of a call that can only
          // time out.
          onCancel={onClose}
        />
      )}
      {stream ? (
        <ScreenSharingOverlay
          room={room}
          bridgeIdentity={stream.bridgeIdentity}
          trackName={stream.trackName}
          width={stream.width}
          height={stream.height}
          onClose={onClose}
        />
      ) : (
        <div className="fixed inset-0 z-50 flex flex-col items-center justify-center gap-3 bg-black/90 text-sm text-white">
          <span data-testid={`host-desktop-overlay-${hostId}-status`} role="status">
            {error ?? `Connecting to ${hostId}…`}
          </span>
          <button
            data-testid={`host-desktop-overlay-${hostId}-close`}
            type="button"
            className="px-3 py-1 text-sm bg-white/10 rounded hover:bg-white/20"
            onClick={onClose}
          >
            Close
          </button>
        </div>
      )}
    </div>,
    document.body,
  );
}

/** The one line of an error worth putting in front of an operator. */
function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
