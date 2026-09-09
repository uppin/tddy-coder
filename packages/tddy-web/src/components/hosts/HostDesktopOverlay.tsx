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
import { ConnectionService, HostPromptKind } from "../../gen/connection_pb";
import {
  Protocol,
  ScreenSharingService,
  type StartStreamResponse,
} from "../../gen/screen_sharing_pb";
import { useAuthContext } from "../../hooks/authProvider";
import { useCommonRoom } from "../../hooks/useCommonRoom";
import { checkHostKey, type KeyPinVerdict } from "../../lib/hostKeyPinning";
import { presenceIdentityForUser } from "../../lib/presenceIdentity";
import { useHostClient } from "../../rpc/connections/registry";
import { useHostPrompts, type HostPromptEventLike } from "../../rpc/useHostPrompts";
import { ScreenSharingOverlay } from "../sessions/ScreenSharingOverlay";
import { HostPassphraseDialog } from "./HostPassphraseDialog";

export interface HostDesktopOverlayProps {
  /** The daemon instance whose desktop this is — host scope's whole addressing change. */
  hostId: string;
  /** The port node 7's probe found serving. Port discovery is out of scope; this is what it checked. */
  port: number;
  /** `screen_sharing.proto`'s `Protocol`, as node 7's reading reports it. */
  protocol: Protocol;
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

/**
 * The host-scoped target for the probed endpoint, creating one the first time.
 *
 * A row has a port and a protocol; `StartHostStream` takes a target id. Matching on the endpoint
 * rather than adding unconditionally is what stops one target accumulating per connect.
 *
 * `sessionToken` is threaded in rather than left to the transport. The daemon gates every
 * host-scoped call on it, and the auth gate every LiveKit-reached host is wrapped in
 * (`rpc/authGatedTransport.ts`) rewrites the field **only where the request already carries one** —
 * so a request that omits it is not quietly repaired on the way out, it is rejected on arrival.
 */
async function targetForProbedEndpoint(
  client: Client<typeof ScreenSharingService>,
  sessionToken: string,
  hostId: string,
  port: number,
  protocol: Protocol,
): Promise<string> {
  const existing = await client.listHostTargets({ sessionToken, daemonInstanceId: hostId });
  const match = existing.targets.find(
    (target) =>
      target.host === PROBED_HOST && target.port === port && target.protocol === protocol,
  );
  if (match) return match.id;

  const added = await client.addHostTarget({
    sessionToken,
    daemonInstanceId: hostId,
    label: PROBED_TARGET_LABEL,
    host: PROBED_HOST,
    port,
    protocol,
    username: "",
  });
  return added.targetId;
}

export function HostDesktopOverlay({ hostId, port, protocol, onClose }: HostDesktopOverlayProps) {
  const client = useHostClient(ScreenSharingService, hostId);
  const connection = useHostClient(ConnectionService, hostId);
  const { user, sessionToken } = useAuthContext();
  const [stream, setStream] = useState<StartStreamResponse | null>(null);
  const [error, setError] = useState<string | null>(null);

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
        const targetId = await targetForProbedEndpoint(
          client,
          sessionTokenRef.current ?? "",
          hostId,
          port,
          protocol,
        );
        if (cancelled) return;
        startedTargetId = targetId;
        // This call blocks while the host asks for the desktop's password — the question arrives
        // below, on the prompt feed, and the answer released the start that is awaited here.
        const started = await client.startHostStream({
          sessionToken: sessionTokenRef.current ?? "",
          daemonInstanceId: hostId,
          targetId,
        });
        if (cancelled) return;
        setStream(started);
      } catch (e) {
        if (cancelled) return;
        setError(messageOf(e));
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
  }, [client, hostId, port, protocol]);

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
  const [question, setQuestion] = useState<HostPromptEventLike | null>(null);

  useEffect(() => {
    if (!awaitingStart) {
      // The start settled — opened or failed — so whatever it was blocked on is no longer being
      // waited for, and a dialog left standing over a desktop asks about nothing.
      setQuestion(null);
      return;
    }
    if (raised === null || raised.kind !== HostPromptKind.DESKTOP_PASSWORD) return;
    // First one wins: the question the operator is answering is not replaced under them.
    setQuestion((held) => held ?? raised);
  }, [awaitingStart, raised]);

  const [continuity, setContinuity] = useState<KeyPinVerdict | null>(null);

  // Checked once per question, after the commit rather than during render: the check *records* the
  // pin, and a render React discards would otherwise spend this host's one first-use trust slot on
  // a dialog nobody was ever shown.
  useEffect(() => {
    setContinuity(
      question === null ? null : checkHostKey(hostId, question.hostPublicKeyFingerprint),
    );
  }, [hostId, question]);

  /** Send the ciphertext the dialog produced. The password itself never reaches this component. */
  const answerWithPassword = async (encryptedAnswer: Uint8Array) => {
    if (question === null || !connection) return;
    try {
      const reply = await connection.answerHostPrompt({
        sessionToken: sessionToken ?? "",
        daemonInstanceId: hostId,
        promptId: question.promptId,
        encryptedAnswer,
      });
      // A refused answer leaves the host waiting on a question it will never get another answer to,
      // so say so now rather than leaving the operator in front of a dialog until the prompt expires.
      if (!reply.accepted) {
        setError(`That password was not accepted: ${reply.rejectionReason}`);
      }
    } catch (e) {
      setError(`The password could not be sent: ${messageOf(e)}`);
    }
  };

  // TODO(#hosts-screen 8/8): forward pointer and keyboard input to the host's bridge (AC-3). The
  // session-scoped overlay does not forward either — `vncInput.ts` is left over from a `VncOverlay`
  // that no longer exists — so this needs a host-scoped input channel rather than a reuse.

  // Portalled to `document.body`, not rendered where it was mounted from. The row section this
  // overlay is opened from is a `<span>` (`HostRowTooling`), and a `<div>` is not permitted inside
  // one: an HTML parser hoists it out on any SSR or hydration path, and even client-side a block
  // element dropped into the row's inline flex flow shifts the row for as long as the overlay is
  // open. `ScreenSharingOverlay` and the connecting state are both `fixed inset-0 z-50` — neither
  // was ever in flow — so the body is where they already behave as though they are.
  return createPortal(
    <div data-testid={`host-desktop-overlay-${hostId}`}>
      {question !== null && continuity !== null && (
        <HostPassphraseDialog
          hostId={hostId}
          subject={question.subject}
          fingerprint={question.hostPublicKeyFingerprint}
          spkiDer={question.hostPublicKey}
          keyContinuity={continuity}
          wording={{
            title: `Password for ${question.subject}`,
            explanation: `${hostId} is waiting for this desktop's password to open it. It is used once and kept nowhere. Leave it blank if this desktop has no password.`,
            placeholder: "Desktop password",
          }}
          // A desktop password is one question an empty answer can be the true answer to: the host
          // asks on every start because it cannot know in advance whether this desktop wants one,
          // and plenty do not. Refusing empty here would ask the operator to invent a password the
          // desktop never had, and leave them with no way in at all.
          allowEmpty
          onSubmit={(encryptedAnswer) => void answerWithPassword(encryptedAnswer)}
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
