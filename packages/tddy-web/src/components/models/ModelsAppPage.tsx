import { ModelsContent } from "tddy-workspace-web";
import { AppShell } from "../shell/AppShell";

/** The Models & Agents route: the application shell around the workspace package's models content. */
export function ModelsAppPage({ onNavigate }: { onNavigate: (path: string) => void }) {
  return (
    <AppShell title="Models & Agents" onNavigate={onNavigate} variant="scroll">
      <ModelsContent />
    </AppShell>
  );
}
