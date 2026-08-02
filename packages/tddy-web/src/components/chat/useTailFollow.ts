/**
 * Sticky-bottom follow for the read-only transcript, in two layers.
 *
 * `useViewportMirror` is pure measurement: the scroll container, the state mirror of what it reports
 * about itself, and the height it reported last. `useTailFollow` is the decision built on top —
 * whether the reader is still following the newest entry, and where the offset has to land when a
 * page is prepended above the loaded range. Neither is useful without the other, so they share a
 * module.
 */

import { useCallback, useLayoutEffect, useRef, useState } from "react";
import {
  isNearTop,
  isPinnedToBottom,
  scrollTopAfterPrepend,
  type ViewportMetrics,
} from "../../lib/scrollFollow";
import type { ChatMessage } from "./useAgentChat";

/** Nothing measured yet — the transcript reports this until its container has been laid out. */
const UNMEASURED_VIEWPORT: ViewportMetrics = { scrollTop: 0, scrollHeight: 0, clientHeight: 0 };

/** How many entries were appended after the one that was newest when the reader detached. Counting
 *  from that entry (rather than from a length) is what makes the count one of *entries*: a tool call
 *  refined from running to completed keeps its key and its place, and a page prepended above the
 *  anchor lands before it, so neither moves the number. */
function arrivedAfter(messages: ChatMessage[], anchorKey: string | null): number {
  if (anchorKey === null) return messages.length;
  const anchorIndex = messages.findIndex((message) => message.key === anchorKey);
  return anchorIndex < 0 ? 0 : messages.length - 1 - anchorIndex;
}

/** The key of the newest entry, or null for an empty range. */
function newestKey(messages: ChatMessage[]): string | null {
  return messages.length > 0 ? messages[messages.length - 1].key : null;
}

/** Did the loaded range grow at its *start* — i.e. was a page paged in? A first render (no previous
 *  key) is the range appearing, not a prepend. */
function isPrepend(previousFirstKey: string | null, firstKey: string | null): boolean {
  return previousFirstKey !== null && firstKey !== null && firstKey !== previousFirstKey;
}

/** Follow the newest entry, or — when a page was just prepended — move the offset by exactly the
 *  height that page added, so the entry being read stays under the same pixel. Anything else leaves
 *  the reader's position alone. */
function applyFollowScroll(
  element: HTMLDivElement,
  args: { pinned: boolean; prepended: boolean; previousScrollHeight: number },
): void {
  if (args.pinned) {
    element.scrollTop = element.scrollHeight;
  } else if (args.prepended) {
    element.scrollTop = scrollTopAfterPrepend({
      scrollTop: element.scrollTop,
      previousScrollHeight: args.previousScrollHeight,
      nextScrollHeight: element.scrollHeight,
    });
  }
}

/**
 * The transcript's scroll container and a state mirror of the metrics it reports about itself. Held
 * apart from the follow decision because it is only measurement: no policy about where the offset
 * ought to be, just what the container currently is and how tall it was when last measured.
 */
function useViewportMirror() {
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const [viewport, setViewport] = useState<ViewportMetrics>(UNMEASURED_VIEWPORT);
  /** The height at the last publish: the baseline a later prepend is measured against. Written here,
   *  read by the follow layout effect. */
  const previousScrollHeightRef = useRef(0);

  /** Mirror the container's live metrics into state (what the hidden scroll-state element renders)
   *  and remember its height, which is the baseline a later prepend is measured against. */
  const publishViewport = useCallback((element: HTMLDivElement): ViewportMetrics => {
    const next: ViewportMetrics = {
      scrollTop: element.scrollTop,
      scrollHeight: element.scrollHeight,
      clientHeight: element.clientHeight,
    };
    previousScrollHeightRef.current = next.scrollHeight;
    setViewport((prev) =>
      prev.scrollTop === next.scrollTop &&
      prev.scrollHeight === next.scrollHeight &&
      prev.clientHeight === next.clientHeight
        ? prev
        : next,
    );
    return next;
  }, []);

  return { scrollRef, viewport, previousScrollHeightRef, publishViewport };
}

interface TailFollowArgs {
  messages: ChatMessage[];
  hasOlder: boolean;
  loadingOlder: boolean;
  onLoadOlder?: () => void;
}

/**
 * Sticky-bottom follow for the read-only transcript: it opens on the newest entry, keeps following
 * while the viewport rests at the bottom, and stops the moment the reader scrolls away — arriving
 * frames then render without moving the read position. Reaching the top of the loaded range asks the
 * host for the page before it, and once that page lands the offset is moved by exactly the height it
 * added, so the entry being read stays under the same pixel.
 */
export function useTailFollow({ messages, hasOlder, loadingOlder, onLoadOlder }: TailFollowArgs) {
  const { scrollRef, viewport, previousScrollHeightRef, publishViewport } = useViewportMirror();
  const [pinned, setPinned] = useState(true);
  /** The entry that was newest when the reader detached; everything after it arrived since. */
  const [detachAnchorKey, setDetachAnchorKey] = useState<string | null>(null);

  // Read inside a scroll handler and a layout effect, both of which run against the latest DOM
  // rather than the render they were created in.
  const pinnedRef = useRef(true);
  const messagesRef = useRef(messages);
  messagesRef.current = messages;
  const previousFirstKeyRef = useRef<string | null>(null);

  const handleScroll = useCallback(() => {
    const element = scrollRef.current;
    if (!element) return;
    const metrics = publishViewport(element);
    const nowPinned = isPinnedToBottom(metrics);

    if (nowPinned) setDetachAnchorKey(null);
    else if (pinnedRef.current) setDetachAnchorKey(newestKey(messagesRef.current));
    pinnedRef.current = nowPinned;
    setPinned(nowPinned);

    // Asking on the gesture (rather than on every render) is what keeps a failed page retryable
    // without retrying it unprompted: the next scroll that reaches the top asks again.
    if (isNearTop(metrics) && hasOlder && !loadingOlder) onLoadOlder?.();
  }, [hasOlder, loadingOlder, onLoadOlder, publishViewport, scrollRef]);

  // Runs before paint, so neither the follow scroll nor the prepend compensation is ever visible as
  // a jump.
  useLayoutEffect(() => {
    const element = scrollRef.current;
    if (!element) return;
    const firstKey = messages.length > 0 ? messages[0].key : null;
    const prepended = isPrepend(previousFirstKeyRef.current, firstKey);
    previousFirstKeyRef.current = firstKey;

    applyFollowScroll(element, {
      pinned: pinnedRef.current,
      prepended,
      previousScrollHeight: previousScrollHeightRef.current,
    });
    publishViewport(element);
  }, [messages, previousScrollHeightRef, publishViewport, scrollRef]);

  const jumpToLatest = useCallback(() => {
    const element = scrollRef.current;
    if (!element) return;
    element.scrollTop = element.scrollHeight;
    pinnedRef.current = true;
    setPinned(true);
    setDetachAnchorKey(null);
    publishViewport(element);
  }, [publishViewport, scrollRef]);

  /** Entries appended since the reader detached; 0 while following. */
  const arrivedWhileDetached = pinned ? 0 : arrivedAfter(messages, detachAnchorKey);
  return { scrollRef, viewport, pinned, arrivedWhileDetached, handleScroll, jumpToLatest };
}
