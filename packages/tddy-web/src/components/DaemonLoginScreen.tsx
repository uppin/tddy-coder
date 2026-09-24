import type { AuthFlowDeclaration } from "../rpc/clientConfig";
import { GitHubLoginButton } from "./GitHubLoginButton";
import { DeviceLoginPanel } from "./DeviceLoginPanel";
import { formClassName } from "./connection/standaloneFormStyles";

/**
 * The sign-in screen a signed-out operator sees on a daemon-mode page.
 *
 * `authFlow` is what the serving daemon declared: `"device"` offers the device-code panel and
 * `"redirect"` the redirect button. Only the declared flow is offered; the other would fail against
 * that daemon. A daemon that declared none serves no sign-in, and one that declared a flow this page
 * does not know is named as an error — neither is ever read as one of the two flows.
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
  authFlow: AuthFlowDeclaration;
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
      <SignInFor authFlow={authFlow} onRedirectLogin={() => login(path)} />
    </div>
  );
}

function SignInFor({
  authFlow,
  onRedirectLogin,
}: {
  authFlow: AuthFlowDeclaration;
  onRedirectLogin: () => void;
}) {
  if (authFlow === "device") return <DeviceLoginPanel />;
  if (authFlow === "redirect") return <GitHubLoginButton onClick={onRedirectLogin} />;
  if (authFlow === "none") {
    return (
      <p data-testid="daemon-login-no-sign-in" className="text-sm text-destructive m-0">
        This daemon has no GitHub sign-in configured.
      </p>
    );
  }
  return (
    <p data-testid="daemon-login-unrecognised-flow" className="text-sm text-destructive m-0">
      This daemon declared a GitHub sign-in flow this dashboard does not know: &quot;{authFlow.unrecognised}&quot;.
    </p>
  );
}
