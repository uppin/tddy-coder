/**
 * Home (connection) screen: cap how many sessions appear per project before overflow (PRD).
 */

/** Max session rows per project on `/` before linking to `/project/:key`. */
export const HOME_PROJECT_SESSIONS_PREVIEW_LIMIT = 10;

export type HomeProjectSessionsPreviewSplit<T> = {
  /** Sessions rendered in the project table on the home screen. */
  visible: T[];
  total: number;
  hiddenCount: number;
};

export function splitSortedSessionsForHomePreview<T>(
  sortedSessions: readonly T[],
): HomeProjectSessionsPreviewSplit<T> {
  const total = sortedSessions.length;
  if (total <= HOME_PROJECT_SESSIONS_PREVIEW_LIMIT) {
    const visible = [...sortedSessions];
    console.debug("[tddy][projectSessionsHomePreview] split: no overflow", { total });
    return { visible, total, hiddenCount: 0 };
  }
  const hiddenCount = total - HOME_PROJECT_SESSIONS_PREVIEW_LIMIT;
  const visible = sortedSessions.slice(0, HOME_PROJECT_SESSIONS_PREVIEW_LIMIT);
  console.info("[tddy][projectSessionsHomePreview] split: capped home preview", {
    total,
    visibleCount: visible.length,
    hiddenCount,
  });
  return { visible, total, hiddenCount };
}
