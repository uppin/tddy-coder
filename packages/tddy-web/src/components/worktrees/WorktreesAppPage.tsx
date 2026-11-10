import { useAuth } from "tddy-client-web";
import { WorktreesContent } from "tddy-workspace-web";
import { AppShell } from "../shell/AppShell";

/**
 * The Worktrees route: the application shell around the workspace package's worktrees content. The
 * signed-out prompt sits in a scrolling shell, the worktree list in a full-bleed one that needs a host.
 */
export function WorktreesAppPage({ onNavigate }: { onNavigate: (path: string) => void }) {
  const { isAuthenticated } = useAuth();
  return isAuthenticated ? (
    <AppShell title="Worktrees" onNavigate={onNavigate} variant="fullbleed" requiresHost>
      <WorktreesContent />
    </AppShell>
  ) : (
    <AppShell title="Worktrees" onNavigate={onNavigate} variant="scroll">
      <WorktreesContent />
    </AppShell>
  );
}
