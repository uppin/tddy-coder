import type { GitHubUser } from "../gen/auth_pb";
import { Button } from "@/components/ui/button";

const containerStyle = {
  display: "flex",
  alignItems: "center",
  gap: 10,
  padding: "8px 0",
} as const;

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
      <Button type="button" variant="outline" size="sm" data-testid="logout-button" onClick={onLogout}>
        Sign out
      </Button>
    </div>
  );
}
