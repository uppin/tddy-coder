import type { GitHubUser } from "../gen/auth_pb";
import { Button } from "@/components/ui/button";
import { useAuthContext } from "../hooks/authProvider";

const containerStyle = {
  display: "flex",
  alignItems: "center",
  gap: 10,
  padding: "8px 0",
} as const;

const refreshingStyle = {
  fontSize: 12,
  color: "#888",
} as const;

/**
 * Unobtrusive indicator shown in the top bar while the shared session store is minting a fresh
 * access token (e.g. right after the device wakes). Renders nothing when no refresh is in flight.
 */
function SessionRefreshIndicator() {
  const { isRefreshing } = useAuthContext();
  if (!isRefreshing) {
    return null;
  }
  return (
    <span data-testid="session-refreshing-indicator" style={refreshingStyle} title="Refreshing session…">
      Refreshing…
    </span>
  );
}

const avatarStyle = {
  width: 32,
  height: 32,
  borderRadius: "50%",
} as const;

export function UserAvatar({
  user,
  onLogout,
}: {
  user: GitHubUser;
  onLogout: () => void;
}) {
  return (
    <div style={containerStyle}>
      <img
        data-testid="user-avatar"
        src={user.avatarUrl}
        alt={user.login}
        style={avatarStyle}
      />
      <span data-testid="user-login">{user.login}</span>
      <SessionRefreshIndicator />
      <Button type="button" variant="outline" size="sm" data-testid="logout-button" onClick={onLogout}>
        Sign out
      </Button>
    </div>
  );
}
