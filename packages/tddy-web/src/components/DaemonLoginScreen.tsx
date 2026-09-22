import type { ClientConfig } from "../rpc/clientConfig";
import { GitHubLoginButton } from "./GitHubLoginButton";
import { DeviceLoginPanel } from "./DeviceLoginPanel";
import { formClassName } from "./connection/standaloneFormStyles";

/**
 * The sign-in screen a signed-out operator sees on a daemon-mode page.
 *
 * `authFlow` is the flow the serving daemon declared: `"device"` offers the device-code panel, and
 * `"redirect"` — or no declaration at all, a daemon that predates the device flow — offers the
 * redirect button. Only the declared flow is offered; the other would fail against that daemon.
 */
export function DaemonLoginScreen({
  path,
  login,
  authError,
  authFlow,
}: {
  path: string;
  login: (returnTo?: string) => void;
  authError: string | null;
  authFlow: ClientConfig["authFlow"];
}) {
  return (
    <div className={`${formClassName} flex flex-col gap-4 pt-12`}>
      <h1 className="text-2xl font-semibold m-0">Sign in</h1>
      <p className="text-sm text-muted-foreground m-0">
        Sign in with GitHub to continue to tddy-web.
      </p>
      {authError ? (
        <p data-testid="auth-flow-error" className="text-sm text-destructive m-0">
          {authError}
        </p>
      ) : null}
      {authFlow === "device" ? <DeviceLoginPanel /> : <GitHubLoginButton onClick={() => login(path)} />}
    </div>
  );
}
