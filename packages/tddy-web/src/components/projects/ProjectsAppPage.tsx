import { ProjectsContent } from "tddy-workspace-web";
import { AppShell } from "../shell/AppShell";

/** The Projects route: the application shell around the workspace package's projects content. */
export function ProjectsAppPage({ onNavigate }: { onNavigate: (path: string) => void }) {
  return (
    <AppShell title="Projects" onNavigate={onNavigate} variant="scroll" requiresHost>
      <ProjectsContent />
    </AppShell>
  );
}
