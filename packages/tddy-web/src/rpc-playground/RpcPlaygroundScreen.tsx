import { useMemo, useState } from "react";
import { Button } from "@/components/ui/button";

export type ServiceMethodKind =
  | "unary"
  | "server_streaming"
  | "client_streaming"
  | "bidi_streaming";

export type ServiceMethod = { name: string; kind: ServiceMethodKind };
export type ServiceInfo = { name: string; methods: ServiceMethod[] };

export type InvokeResult =
  | { kind: "success"; json: string }
  | { kind: "error"; code: string; message: string }
  | { kind: "stream_complete"; chunks: string[] };

export type ParticipantOption = { id: string; label: string };

export interface RpcPlaygroundScreenProps {
  services: ServiceInfo[];
  /** Optional participant/host selector options. */
  participants?: ParticipantOption[];
  selectedParticipant?: string;
  onSelectParticipant?: (id: string) => void;
  onInvoke: (
    serviceName: string,
    methodName: string,
    requestJson: string,
  ) => Promise<InvokeResult>;
  onNavigate: (path: string) => void;
}

const KIND_LABELS: Record<ServiceMethodKind, string> = {
  unary: "unary",
  server_streaming: "server stream",
  client_streaming: "client stream",
  bidi_streaming: "bidi stream",
};

const selectClassName =
  "box-border min-w-[12rem] max-w-[24rem] rounded-md border border-input bg-background px-2 py-1.5 text-sm text-foreground shadow-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring";

const screenShellClassName =
  "min-h-svh w-full min-w-0 box-border px-4 py-6 sm:px-6 font-sans text-foreground";

type EditorMode = "builder" | "raw";

/**
 * Presentational RPC Playground: pick a service + method, edit a JSON request, invoke it,
 * and inspect the response or error. All data flows in via props (no transport here).
 */
export function RpcPlaygroundScreen({
  services,
  participants,
  selectedParticipant,
  onSelectParticipant,
  onInvoke,
  onNavigate,
}: RpcPlaygroundScreenProps) {
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const [selectedService, setSelectedService] = useState<string | null>(null);
  const [selectedMethod, setSelectedMethod] = useState<string | null>(null);
  const [editorMode, setEditorMode] = useState<EditorMode>("raw");
  const [requestJson, setRequestJson] = useState("{}");
  const [result, setResult] = useState<InvokeResult | null>(null);
  const [invoking, setInvoking] = useState(false);

  const currentMethod = useMemo(() => {
    if (!selectedService || !selectedMethod) return null;
    const svc = services.find((s) => s.name === selectedService);
    return svc?.methods.find((m) => m.name === selectedMethod) ?? null;
  }, [services, selectedService, selectedMethod]);

  const toggleService = (name: string) => {
    setExpanded((prev) => ({ ...prev, [name]: !prev[name] }));
  };

  const selectMethod = (serviceName: string, methodName: string) => {
    setSelectedService(serviceName);
    setSelectedMethod(methodName);
    setRequestJson("{}");
    setResult(null);
  };

  const handleInvoke = async () => {
    if (!selectedService || !selectedMethod) return;
    setInvoking(true);
    setResult(null);
    try {
      const res = await onInvoke(selectedService, selectedMethod, requestJson);
      setResult(res);
    } catch (e) {
      setResult({ kind: "error", code: "unknown", message: String(e) });
    } finally {
      setInvoking(false);
    }
  };

  return (
    <div className={screenShellClassName}>
      <div className="flex flex-wrap items-center justify-between gap-4">
        <h1 className="text-2xl font-semibold">RPC Playground</h1>
        <Button
          type="button"
          variant="secondary"
          data-testid="rpc-playground-home"
          onClick={() => onNavigate("/")}
        >
          Home
        </Button>
      </div>

      <div className="mt-4 flex min-w-[10rem] flex-col gap-1">
        <label className="text-sm font-medium" htmlFor="rpc-participant">
          Participant / host
        </label>
        <select
          id="rpc-participant"
          data-testid="rpc-playground-participant-select"
          className={selectClassName}
          value={selectedParticipant ?? ""}
          onChange={(e) => onSelectParticipant?.(e.target.value)}
        >
          {(participants ?? []).length === 0 ? (
            <option value="">—</option>
          ) : null}
          {(participants ?? []).map((p) => (
            <option key={p.id} value={p.id}>
              {p.label}
            </option>
          ))}
        </select>
      </div>

      <div className="mt-6 grid grid-cols-1 gap-6 md:grid-cols-[18rem_1fr]">
        <div data-testid="rpc-service-tree" className="flex flex-col gap-1">
          {services.map((svc) => (
            <div key={svc.name} className="rounded-md border border-input">
              <button
                type="button"
                data-testid={`rpc-service-${svc.name}`}
                className="w-full px-3 py-2 text-left text-sm font-medium"
                onClick={() => toggleService(svc.name)}
              >
                {expanded[svc.name] ? "▾" : "▸"} {svc.name}
              </button>
              {expanded[svc.name] ? (
                <ul className="border-t border-input">
                  {svc.methods.map((m) => {
                    const active =
                      selectedService === svc.name && selectedMethod === m.name;
                    return (
                      <li key={m.name}>
                        <button
                          type="button"
                          data-testid={`rpc-method-${svc.name}-${m.name}`}
                          className={`flex w-full items-center justify-between gap-2 px-3 py-1.5 text-left text-sm ${active ? "bg-accent" : ""}`}
                          onClick={() => selectMethod(svc.name, m.name)}
                        >
                          <span>{m.name}</span>
                          <span
                            data-testid={`rpc-method-kind-${svc.name}-${m.name}`}
                            className="rounded bg-muted px-1.5 py-0.5 text-xs text-muted-foreground"
                          >
                            {KIND_LABELS[m.kind]}
                          </span>
                        </button>
                      </li>
                    );
                  })}
                </ul>
              ) : null}
            </div>
          ))}
        </div>

        <div className="flex min-w-0 flex-col gap-4">
          {currentMethod ? (
            <div data-testid="rpc-request-editor" className="flex flex-col gap-2">
              <div className="flex items-center gap-2">
                <span className="text-sm font-medium">
                  {selectedService}/{selectedMethod}
                </span>
                <span className="rounded bg-muted px-1.5 py-0.5 text-xs text-muted-foreground">
                  {KIND_LABELS[currentMethod.kind]}
                </span>
              </div>
              <div className="flex gap-2">
                <Button
                  type="button"
                  variant={editorMode === "builder" ? "default" : "secondary"}
                  data-testid="rpc-editor-toggle-builder"
                  onClick={() => setEditorMode("builder")}
                >
                  Builder
                </Button>
                <Button
                  type="button"
                  variant={editorMode === "raw" ? "default" : "secondary"}
                  data-testid="rpc-editor-toggle-raw"
                  onClick={() => setEditorMode("raw")}
                >
                  Raw JSON
                </Button>
              </div>
              <textarea
                data-testid="rpc-request-raw-json"
                className="min-h-[10rem] w-full rounded-md border border-input bg-background p-2 font-mono text-sm"
                value={requestJson}
                onChange={(e) => setRequestJson(e.target.value)}
                spellCheck={false}
              />
              <div>
                <Button
                  type="button"
                  data-testid="rpc-invoke-button"
                  disabled={invoking}
                  onClick={() => void handleInvoke()}
                >
                  {invoking ? "Invoking…" : "Invoke"}
                </Button>
              </div>
            </div>
          ) : (
            <p className="text-sm text-muted-foreground">
              Select a method to build and invoke a request.
            </p>
          )}

          {result?.kind === "error" ? (
            <div
              data-testid="rpc-error"
              className="rounded-md border border-destructive bg-destructive/10 p-3 text-sm text-destructive"
            >
              <div className="font-semibold">Error: {result.code}</div>
              <div>{result.message}</div>
            </div>
          ) : null}

          {result?.kind === "success" ? (
            <pre
              data-testid="rpc-response"
              className="overflow-auto rounded-md border border-input bg-muted p-3 font-mono text-xs"
            >
              {result.json}
            </pre>
          ) : null}

          {result?.kind === "stream_complete" ? (
            <pre
              data-testid="rpc-response"
              className="overflow-auto rounded-md border border-input bg-muted p-3 font-mono text-xs"
            >
              {result.chunks.join("\n---\n")}
            </pre>
          ) : null}
        </div>
      </div>
    </div>
  );
}
