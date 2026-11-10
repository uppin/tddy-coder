import { useEffect, useRef, useState, type MutableRefObject, type RefObject } from "react";
import type { TerminalFeed } from "tddy-client-web/connections/terminal";
import { TerminalStreamOffset } from "../lib/terminalStreamOffset";
import type { ByteDelta } from "../lib/byteDelta";
import type { TerminalViewHandle } from "../terminal/types";
import { dHistory, dTerm, hexPreview } from "./sessionDiagnostics";

export interface FeedStreamInput {
  feed: TerminalFeed;
  sessionId: string | undefined;
  debugMode: boolean;
  showBufferTextForTest: boolean;
  onOffsetUpdate: ((offset: bigint) => void) | undefined;
  onBytesRef: MutableRefObject<((delta: ByteDelta) => void) | undefined>;
  termRef: RefObject<TerminalViewHandle | null>;
  termReadyRef: MutableRefObject<boolean>;
  outputBufferRef: MutableRefObject<Uint8Array[]>;
  streamedTextRef: MutableRefObject<string>;
  setStreamedByteCount: (update: (count: number) => number) => void;
  /** Cumulative output offset received, carried across stream opens. */
  currentOffsetRef: MutableRefObject<bigint>;
  anchorRef: MutableRefObject<{ endOffset: bigint; atOldest: boolean } | null>;
  setAnchor: (anchor: { endOffset: bigint; atOldest: boolean }) => void;
  noteLiveModeSwitches: (term: TerminalViewHandle, data: Uint8Array) => void;
}

/**
 * Consumes the feed's output stream: counts bytes, captures the lazy-history anchor, tracks the
 * received offset, and writes to the live terminal — or holds chunks until it is ready.
 */
export function useFeedStream({
  feed,
  sessionId,
  debugMode,
  showBufferTextForTest,
  onOffsetUpdate,
  onBytesRef,
  termRef,
  termReadyRef,
  outputBufferRef,
  streamedTextRef,
  setStreamedByteCount,
  currentOffsetRef,
  anchorRef,
  setAnchor,
  noteLiveModeSwitches,
}: FeedStreamInput) {
  const [firstOutputReceived, setFirstOutputReceived] = useState(false);
  const [rpcReceivedCount, setRpcReceivedCount] = useState(0);
  const [rpcReceivedSample, setRpcReceivedSample] = useState("");
  const onOffsetUpdateRef = useRef(onOffsetUpdate);
  onOffsetUpdateRef.current = onOffsetUpdate;

  useEffect(() => {
    // One accounting instance per feed, seeded with what the previous one had received: frames that
    // precede this open's offset-anchored frame (the re-issued mode prologue) are replayed VT state,
    // not stream bytes, and must not move the offset.
    const streamOffset = new TerminalStreamOffset(currentOffsetRef.current);
    let received = 0;
    feed.stream.onMessage((frame) => {
      const data = frame.data;
      received += 1;
      onBytesRef.current?.({ bytesIn: data.length });
      if (received === 1) setFirstOutputReceived(true);
      // Capture the lazy-history anchor from the initial replay frame (endOffset > 0). The anchor
      // drives the forward fill of the older-history terminal; storing it in state ensures the
      // affordance re-renders immediately when it is captured. Until a frame reports an offset past
      // zero there is none, and a fill started then runs unanchored — see `startForwardFill`.
      if (frame.endOffset > 0n && anchorRef.current === null) {
        const a = { endOffset: frame.endOffset, atOldest: frame.atOldest };
        anchorRef.current = a;
        setAnchor(a);
        dHistory(
          "lazy history anchor endOffset=%s atOldest=%o sessionId=%s",
          frame.endOffset.toString(),
          frame.atOldest,
          sessionId ?? "",
        );
      }
      // Advance the cumulative output offset and surface it to the parent so a reconnect resumes
      // with `FROM_OFFSET` (no duplicate replay).
      currentOffsetRef.current = streamOffset.accept(frame);
      onOffsetUpdateRef.current?.(currentOffsetRef.current);

      if (debugMode) {
        setRpcReceivedCount(received);
        setRpcReceivedSample(new TextDecoder().decode(data.slice(0, 200)));
        return;
      }
      if (showBufferTextForTest) {
        streamedTextRef.current += new TextDecoder().decode(data);
        setStreamedByteCount((count) => count + data.length);
      }
      const ready = termReadyRef.current && !!termRef.current;
      if (dTerm.enabled) {
        dTerm("recv %d bytes ready=%o %s", data.length, ready, hexPreview(data));
      }
      if (ready && termRef.current) {
        termRef.current.write(data);
        noteLiveModeSwitches(termRef.current, data);
      } else {
        // The reconnect byte-buffering fix: bytes that arrive before xterm.js is ready are held
        // and flushed in order on `onReady`, instead of being written into a terminal that is not
        // there (which is what garbled a 220-column replay).
        outputBufferRef.current.push(data);
      }
    });
    return () => {
      feed.stream.close();
    };
  }, [feed, debugMode, showBufferTextForTest]);

  return { firstOutputReceived, rpcReceivedCount, rpcReceivedSample };
}

/**
 * The far end going away: the feed settled its `ended`. Input stops going out (there is nothing
 * reading it) and the pane says so, rather than looking interactive over a dead PTY.
 */
export function useSessionEnded(
  feed: TerminalFeed,
  onRemoteSessionEnded: (() => void) | undefined,
) {
  const [sessionEnded, setSessionEnded] = useState(false);
  const sessionEndedRef = useRef(false);
  const onRemoteSessionEndedRef = useRef(onRemoteSessionEnded);
  onRemoteSessionEndedRef.current = onRemoteSessionEnded;

  // The far end going away. `ended` never rejects and a feed that cannot tell simply never settles
  // it, so a terminal on such a wire keeps tailing rather than being told a lie.
  useEffect(() => {
    if (!feed.ended) return;
    let watching = true;
    void feed.ended.then(() => {
      if (!watching) return;
      sessionEndedRef.current = true;
      setSessionEnded(true);
      onRemoteSessionEndedRef.current?.();
    });
    return () => {
      watching = false;
    };
  }, [feed]);

  return { sessionEnded, sessionEndedRef };
}
