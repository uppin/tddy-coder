const buttonStyle = {
  padding: "10px 20px",
  fontSize: 14,
  cursor: "pointer",
  backgroundColor: "#24292e",
  color: "#fff",
  border: "none",
  borderRadius: 6,
  fontWeight: 500,
} as const;

export function GitHubLoginButton({ onClick }: { onClick: () => void }) {
  return (
    <button
      data-testid="github-login-button"
      onClick={onClick}
      style={buttonStyle}
    >
      Sign in with GitHub
    </button>
  );
}
