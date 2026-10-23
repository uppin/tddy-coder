import React from "react";
import { cn } from "@/lib/utils";
import { useHostConnection } from "../../rpc/connections/registry";
import { useHostDirectorySource } from "../../rpc/hostDirectory/useHostDirectory";
import { LIVEKIT_SOURCE_ID } from "../../rpc/hostDirectory/liveKitSource";
import { useSelectedDaemon } from "../../rpc/selectedDaemon";
import { hostGateOf } from "./hostGate";

/**
 * Covers a host-scoped screen's body while no viable RPC client reaches the selected host, naming
 * the host and the reason (`hostGateOf`). The body stays mounted underneath — `inert`, so nothing in
 * it can be reached — so a connection that comes back resumes the screen as it was rather than
 * remounting it. The header above is not covered: the host selector and navigation stay usable.
 */
export function HostConnectionGate({
  className,
  children,
}: {
  /** Layout classes for the wrapper, so it takes the place the body had in the shell. */
  className?: string;
  children: React.ReactNode;
}) {
  const { selectedInstanceId, selectedLabel } = useSelectedDaemon();
  const connection = useHostConnection(selectedInstanceId);
  const liveKit = useHostDirectorySource(LIVEKIT_SOURCE_ID);
  const gate = hostGateOf({ selectedInstanceId, hostLabel: selectedLabel, connection, liveKit });
  const blocked = gate.kind === "blocked";

  return (
    <div className={cn("relative", className)}>
      <div
        data-testid="host-connection-gated-body"
        className={cn("contents", blocked && "pointer-events-none select-none")}
        inert={blocked || undefined}
        aria-hidden={blocked || undefined}
      >
        {children}
      </div>
      {blocked ? (
        <div
          data-testid="host-connection-gate"
          role={gate.tone === "error" ? "alert" : "status"}
          className="absolute inset-0 z-40 flex items-start justify-center bg-background/90 p-6 backdrop-blur-sm"
        >
          <div className="mt-12 max-w-md rounded-md border border-border bg-background p-4 shadow-md">
            <p
              data-testid="host-connection-gate-title"
              className={cn("font-semibold", gate.tone === "error" ? "text-destructive" : "text-foreground")}
            >
              {gate.title}
            </p>
            {gate.reason ? (
              <p data-testid="host-connection-gate-reason" className="mt-2 text-sm text-muted-foreground">
                {gate.reason}
              </p>
            ) : null}
          </div>
        </div>
      ) : null}
    </div>
  );
}
