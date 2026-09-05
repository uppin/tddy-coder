# 2026-08-02 — The activity transcript opens at the end, follows it, and pages back on demand

- **The transcript now opens on the newest entry.** It used to open on the oldest one — the least interesting thing a finished session did — so reading what the agent last did meant scrolling to the bottom by hand, every time you opened the pane and every time you switched sessions and back.
- **It follows the agent while you are at the bottom.** New activity scrolls into view on its own; previously it appended below the fold and the pane silently stopped reflecting what the agent was doing.
- **Scrolling up stops it moving.** Frames still arrive and still render, but the view stays where you left it. A button appears saying how many entries arrived while you were reading; clicking it, or scrolling back down, returns you to the newest entry.
- **Older history loads as you scroll back**, a page at a time, and the entry you were reading stays under the same pixel when it lands. Reaching the beginning stops the fetching for good.
- **Opening a session no longer transfers its whole recorded history.** Only the newest 100 entries are fetched up front, so a session with a large recording costs a page rather than all of it to show the end.
- **A failed page load leaves the transcript alone** — nothing invented, nothing half-loaded, and no false "you have reached the beginning". Scrolling up again retries.
- **Switching sessions no longer carries one session's scroll position into another.**
