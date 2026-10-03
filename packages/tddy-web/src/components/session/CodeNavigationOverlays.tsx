import type { CodeLocationRef } from "./codeNavigationApi";
import type { Hover, LocationList, Notice } from "./useCodeNavigation";

export function NavigationNotice({ notice }: { notice: Notice }) {
  return (
    <p
      role={notice.isError ? "alert" : "status"}
      className={`pointer-events-auto rounded border border-border bg-background p-2 text-sm ${
        notice.isError ? "text-destructive" : "text-muted-foreground"
      }`}
    >
      {notice.text}
    </p>
  );
}

export function HoverCard({
  hover,
  onShowReferences,
  onClose,
}: {
  hover: Hover;
  onShowReferences: () => void;
  onClose: () => void;
}) {
  return (
    <div
      data-testid="worktree-code-hover"
      className="pointer-events-auto rounded border border-border bg-background p-2 shadow"
    >
      <pre className="whitespace-pre-wrap font-mono text-sm">{hover.markdown}</pre>
      <div className="mt-2 flex gap-2">
        <button
          type="button"
          data-testid="worktree-code-references-action"
          className="text-sm underline"
          onClick={onShowReferences}
        >
          References
        </button>
        <button type="button" className="text-sm underline" onClick={onClose}>
          Close
        </button>
      </div>
    </div>
  );
}

export function LocationListCard({
  list,
  onOpen,
}: {
  list: LocationList;
  onOpen: (location: CodeLocationRef) => void;
}) {
  return (
    <div
      data-testid="worktree-code-references"
      className="pointer-events-auto max-h-48 overflow-auto rounded border border-border bg-background p-2 shadow"
    >
      <p className="text-sm font-medium">{list.title}</p>
      {list.locations.length === 0 && (
        <p className="text-sm text-muted-foreground">None found</p>
      )}
      <ul>
        {list.locations.map((location) => (
          <li key={`${location.relPath}:${location.start.line}:${location.start.column}`}>
            <button
              type="button"
              data-testid={`worktree-code-reference-${location.relPath}-${location.start.line}`}
              disabled={location.outsideWorktree}
              className="text-left font-mono text-sm underline disabled:cursor-not-allowed disabled:no-underline disabled:opacity-60"
              onClick={() => onOpen(location)}
            >
              {location.relPath}:{location.start.line}
              {location.outsideWorktree ? " (outside the worktree)" : ""}
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
