import React, { useEffect, useState } from "react";
import type { ToolDef } from "../../gen/connection_pb";
import { defaultArgsFromSchema } from "./toolSchema";

interface ToolCallInfo {
  taskId: string;
  toolName: string;
  argsJson: string;
  resultJson: string;
  isError: boolean;
  errorMessage: string;
  jobRunning: boolean;
  createdUnixMs: bigint;
}

interface InvokeResult {
  resultJson: string;
  isError: boolean;
  errorMessage: string;
}

interface SessionToolsTabProps {
  sessionId: string;
  onListExecTools: () => Promise<Partial<ToolDef>[]>;
  onListSessionToolCalls: () => Promise<ToolCallInfo[]>;
  onExecuteTool: (args: {
    sessionId: string;
    toolName: string;
    argsJson: string;
  }) => Promise<InvokeResult>;
}

function extractStdio(resultJson: string): string | null {
  try {
    const parsed = JSON.parse(resultJson) as Record<string, unknown>;
    if ("stdout" in parsed || "stderr" in parsed) {
      const parts: string[] = [];
      if (parsed.stdout && parsed.stdout !== "") {
        parts.push(`stdout:\n${String(parsed.stdout)}`);
      }
      if (parsed.stderr && parsed.stderr !== "") {
        parts.push(`stderr:\n${String(parsed.stderr)}`);
      }
      if ("exit_code" in parsed) {
        parts.push(`exit_code: ${String(parsed.exit_code)}`);
      }
      return parts.length > 0 ? parts.join("\n\n") : null;
    }
  } catch {
    // not JSON or not a shell result
  }
  return null;
}

export function SessionToolsTab({
  sessionId,
  onListExecTools,
  onListSessionToolCalls,
  onExecuteTool,
}: SessionToolsTabProps) {
  const [tools, setTools] = useState<Partial<ToolDef>[]>([]);
  const [selectedTool, setSelectedTool] = useState<string>("");
  const [argsJson, setArgsJson] = useState<string>("{}");
  const [invokeResult, setInvokeResult] = useState<InvokeResult | null>(null);
  const [invoking, setInvoking] = useState(false);
  const [callLog, setCallLog] = useState<ToolCallInfo[]>([]);
  const [expandedRows, setExpandedRows] = useState<Set<number>>(new Set());

  useEffect(() => {
    let cancelled = false;
    onListExecTools()
      .then((t) => {
        if (!cancelled) {
          setTools(t);
          if (t.length > 0 && t[0].name) {
            setSelectedTool(t[0].name);
            setArgsJson(defaultArgsFromSchema(t[0].inputSchemaJson ?? "{}"));
          }
        }
      })
      .catch(() => {
        /* ignore */
      });
    return () => {
      cancelled = true;
    };
  }, [onListExecTools]);

  useEffect(() => {
    let cancelled = false;
    onListSessionToolCalls()
      .then((calls) => {
        if (!cancelled) setCallLog(calls);
      })
      .catch(() => {
        /* ignore */
      });
    return () => {
      cancelled = true;
    };
  }, [onListSessionToolCalls]);

  const handleToolSelect = (toolName: string) => {
    setSelectedTool(toolName);
    const tool = tools.find((t) => t.name === toolName);
    setArgsJson(defaultArgsFromSchema(tool?.inputSchemaJson ?? "{}"));
    setInvokeResult(null);
  };

  const handleInvoke = async () => {
    setInvoking(true);
    setInvokeResult(null);
    try {
      const result = await onExecuteTool({ sessionId, toolName: selectedTool, argsJson });
      setInvokeResult(result);
      try {
        const calls = await onListSessionToolCalls();
        setCallLog(calls);
      } catch {
        /* ignore */
      }
    } catch (e) {
      setInvokeResult({ resultJson: "", isError: true, errorMessage: String(e) });
    } finally {
      setInvoking(false);
    }
  };

  const toggleRow = (index: number) => {
    setExpandedRows((prev) => {
      const next = new Set(prev);
      if (next.has(index)) {
        next.delete(index);
      } else {
        next.add(index);
      }
      return next;
    });
  };

  const reversedLog = [...callLog].reverse();

  return (
    <div data-testid="sessions-inspector-tools-panel" className="flex flex-col gap-4 px-3 py-3">
      {/* Invoke Panel */}
      <div className="flex flex-col gap-2">
        <span className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
          Invoke Tool
        </span>
        <select
          data-testid="sessions-tool-invoke-select"
          value={selectedTool}
          onChange={(e) => handleToolSelect(e.target.value)}
          className="text-xs border border-input rounded px-2 py-1 bg-background text-foreground"
        >
          {tools.map((t) => (
            <option key={t.name} value={t.name}>
              {t.name}
            </option>
          ))}
        </select>
        <textarea
          data-testid="sessions-tool-invoke-args"
          value={argsJson}
          onChange={(e) => setArgsJson(e.target.value)}
          className="text-xs font-mono border border-input rounded px-2 py-1 bg-background text-foreground min-h-[80px] resize-y"
        />
        <button
          data-testid="sessions-tool-invoke-button"
          onClick={handleInvoke}
          disabled={invoking || !selectedTool}
          className="text-xs px-3 py-1 rounded bg-foreground text-background hover:opacity-80 disabled:opacity-50"
        >
          {invoking ? "Invoking…" : "Invoke"}
        </button>
        {invokeResult && !invokeResult.isError && (
          <pre
            data-testid="sessions-tool-invoke-result"
            className="text-xs font-mono bg-muted rounded p-2 overflow-x-auto whitespace-pre-wrap"
          >
            {invokeResult.resultJson}
          </pre>
        )}
        {invokeResult && invokeResult.isError && (
          <div
            data-testid="sessions-tool-invoke-error"
            className="text-xs text-destructive bg-destructive/10 rounded p-2"
          >
            {invokeResult.errorMessage}
          </div>
        )}
      </div>

      {/* Call Log */}
      <div className="flex flex-col gap-1">
        <span className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
          Call Log
        </span>
        <div data-testid="sessions-tool-call-log" className="flex flex-col gap-1">
          {reversedLog.length === 0 ? (
            <span className="text-xs text-muted-foreground">
              No tool calls recorded for this session yet.
            </span>
          ) : (
            reversedLog.map((call, displayIndex) => {
              const originalIndex = callLog.length - 1 - displayIndex;
              const isExpanded = expandedRows.has(originalIndex);
              const stdio = extractStdio(call.resultJson);
              return (
                <div
                  key={String(call.taskId) || String(originalIndex)}
                  className="border border-border rounded"
                >
                  <div
                    data-testid={`sessions-tool-call-row`}
                    onClick={() => toggleRow(originalIndex)}
                    className="flex items-center gap-2 px-2 py-1 cursor-pointer hover:bg-muted text-xs"
                  >
                    <span className="font-medium">{call.toolName}</span>
                    {call.isError && <span className="text-destructive">[error]</span>}
                    {call.jobRunning && (
                      <span className="text-muted-foreground">[running]</span>
                    )}
                  </div>
                  {isExpanded && (
                    <div className="flex flex-col gap-1 px-2 py-1 border-t border-border">
                      <pre
                        data-testid="sessions-tool-call-input"
                        className="text-xs font-mono bg-muted rounded p-1 overflow-x-auto whitespace-pre-wrap"
                      >
                        {call.argsJson}
                      </pre>
                      <pre
                        data-testid="sessions-tool-call-output"
                        className="text-xs font-mono bg-muted rounded p-1 overflow-x-auto whitespace-pre-wrap"
                      >
                        {call.resultJson}
                      </pre>
                      {stdio !== null && (
                        <pre
                          data-testid="sessions-tool-call-stdio"
                          className="text-xs font-mono bg-muted rounded p-1 overflow-x-auto whitespace-pre-wrap"
                        >
                          {stdio}
                        </pre>
                      )}
                    </div>
                  )}
                </div>
              );
            })
          )}
        </div>
      </div>
    </div>
  );
}
