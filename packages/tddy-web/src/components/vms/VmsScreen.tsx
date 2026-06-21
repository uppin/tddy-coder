import { Button } from "@/components/ui/button";

export interface VmRow {
  name: string;
  state: string;
  sshHostPort: number;
  shareUrl: string;
  errorMessage: string;
}

export interface VmsScreenProps {
  rows: VmRow[];
  onStart: (name: string) => void;
  onStop: (name: string) => void;
  onRemove: (name: string) => void;
}

export function VmsScreen({ rows, onStart, onStop, onRemove }: VmsScreenProps) {
  return (
    <div data-testid="vms-screen">
      <h2 className="text-lg font-semibold mb-4">VMs</h2>
      {rows.length === 0 ? (
        <p data-testid="vms-empty" className="text-muted-foreground text-sm">
          No VMs defined.
        </p>
      ) : (
        <table data-testid="vms-table" className="w-full text-sm border-collapse">
          <thead>
            <tr>
              <th className="text-left py-2 pr-4">Name</th>
              <th className="text-left py-2 pr-4">State</th>
              <th className="text-left py-2 pr-4">SSH Port</th>
              <th className="text-left py-2 pr-4">Share URL</th>
              <th className="text-left py-2">Actions</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <tr key={row.name} data-testid={`vms-row-${row.name}`}>
                <td className="py-2 pr-4">{row.name}</td>
                <td className="py-2 pr-4" data-testid={`vms-state-${row.name}`}>{row.state}</td>
                <td className="py-2 pr-4">{row.sshHostPort > 0 ? row.sshHostPort : "—"}</td>
                <td className="py-2 pr-4">
                  {row.shareUrl ? (
                    <a href={row.shareUrl} target="_blank" rel="noreferrer">{row.shareUrl}</a>
                  ) : "—"}
                </td>
                <td className="py-2 flex gap-2">
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    data-testid={`vms-start-${row.name}`}
                    onClick={() => onStart(row.name)}
                  >
                    Start
                  </Button>
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    data-testid={`vms-stop-${row.name}`}
                    onClick={() => onStop(row.name)}
                  >
                    Stop
                  </Button>
                  <Button
                    type="button"
                    size="sm"
                    variant="destructive"
                    data-testid={`vms-remove-${row.name}`}
                    onClick={() => onRemove(row.name)}
                  >
                    Remove
                  </Button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
