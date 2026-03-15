import type { GitHubUser } from "../gen/auth_pb";

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

const logoutButtonStyle = {
  padding: "4px 10px",
  fontSize: 12,
  cursor: "pointer",
  backgroundColor: "transparent",
  border: "1px solid #ccc",
  borderRadius: 4,
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
      <button
        data-testid="logout-button"
        onClick={onLogout}
        style={logoutButtonStyle}
      >
        Sign out
      </button>
    </div>
  );
}
