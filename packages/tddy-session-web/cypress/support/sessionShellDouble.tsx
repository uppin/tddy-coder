/**
 * The screen shell a drawer spec mounts a session screen into: a header with the title and the
 * screen's own controls, and a body — without the app's navigation menu, daemon selector or avatar,
 * which are the application's. `fullbleed` keeps the real shell's fixed-height, overflow-hidden box,
 * since the drawer's layout (and its viewport specs) depend on the body owning the scroll.
 */
import React from "react";
import type { SessionShellProps } from "../../src/sessionShell";

export function SessionShellDouble({ title, variant = "scroll", headerRight, dataTestId, children }: SessionShellProps) {
  const header = (
    <div className="flex items-center gap-2 px-2 py-0.5 border-b border-border flex-shrink-0">
      <h1 className="text-sm font-semibold truncate">{title}</h1>
      <div className="flex-1" />
      {headerRight}
    </div>
  );
  if (variant === "fullbleed") {
    return (
      <div data-testid={dataTestId} className="flex flex-col h-[100dvh] w-full overflow-hidden font-sans text-foreground">
        {header}
        <div className="flex flex-col flex-1 min-h-0 overflow-hidden">{children}</div>
      </div>
    );
  }
  return (
    <div data-testid={dataTestId} className="min-h-screen w-full font-sans text-foreground">
      {header}
      <div className="p-4">{children}</div>
    </div>
  );
}
