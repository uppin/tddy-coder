import { Button } from "@/components/ui/button";

export function GitHubLoginButton({ onClick }: { onClick: () => void }) {
  return (
    <Button
      type="button"
      data-testid="github-login-button"
      onClick={onClick}
      className="bg-[#24292f] text-white hover:bg-[#24292f]/90 dark:bg-[#24292f] dark:hover:bg-[#24292f]/90"
    >
      Sign in with GitHub
    </Button>
  );
}
