import { Button } from "@/components/ui/button";

/** GitHub's own dark button colours, shared by every "Sign in with GitHub" button. */
export const GITHUB_BUTTON_CLASS_NAME =
  "bg-[#24292f] text-white hover:bg-[#24292f]/90 dark:bg-[#24292f] dark:hover:bg-[#24292f]/90";

export function GitHubLoginButton({ onClick }: { onClick: () => void }) {
  return (
    <Button
      type="button"
      data-testid="github-login-button"
      onClick={onClick}
      className={GITHUB_BUTTON_CLASS_NAME}
    >
      Sign in with GitHub
    </Button>
  );
}
