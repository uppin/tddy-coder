import { Button } from "@/components/ui/button";
import { useAuthContext } from "../hooks/authProvider";

/**
 * GitHub sign-in by device code, for a daemon that serves `StartDeviceLogin` / `PollDeviceLogin`
 * in place of the redirect flow (a public `client_id` and no client secret — Tddy Desktop).
 *
 * The operator asks for a code, types it at GitHub's verification page, and approves there; the
 * shared auth context polls the daemon until GitHub answers and signs the operator in. A refused or
 * expired attempt is named as such and offers a fresh code.
 */
export function DeviceLoginPanel() {
  const { deviceLogin, startDeviceLogin } = useAuthContext();

  if (deviceLogin.phase === "awaiting-approval") {
    return (
      <div className="flex flex-col gap-2">
        <p className="text-sm text-muted-foreground m-0">
          Open{" "}
          <a
            data-testid="device-login-verification-link"
            href={deviceLogin.verificationUri}
            target="_blank"
            rel="noreferrer"
            className="underline"
          >
            {deviceLogin.verificationUri}
          </a>{" "}
          and enter this code:
        </p>
        <p data-testid="device-login-user-code" className="text-2xl font-mono font-semibold tracking-widest m-0">
          {deviceLogin.userCode}
        </p>
        <p className="text-sm text-muted-foreground m-0">Waiting for approval at GitHub…</p>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-2">
      {deviceLogin.phase === "denied" ? (
        <p data-testid="device-login-denied" className="text-sm text-destructive m-0">
          Sign-in was denied at GitHub.
        </p>
      ) : null}
      {deviceLogin.phase === "expired" ? (
        <p data-testid="device-login-expired" className="text-sm text-destructive m-0">
          The sign-in code expired before it was approved.
        </p>
      ) : null}
      {deviceLogin.phase === "failed" ? (
        <p data-testid="device-login-error" className="text-sm text-destructive m-0">
          {deviceLogin.error}
        </p>
      ) : null}
      <Button
        type="button"
        data-testid="device-login-start"
        disabled={deviceLogin.phase === "starting"}
        onClick={() => void startDeviceLogin()}
        className="bg-[#24292f] text-white hover:bg-[#24292f]/90 dark:bg-[#24292f] dark:hover:bg-[#24292f]/90"
      >
        {deviceLogin.phase === "idle" || deviceLogin.phase === "starting"
          ? "Sign in with GitHub"
          : "Get a new code"}
      </Button>
    </div>
  );
}
