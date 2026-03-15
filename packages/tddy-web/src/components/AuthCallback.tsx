import { useEffect, useState } from "react";
import { useAuth } from "../hooks/useAuth";

export function AuthCallback() {
  const { handleCallback, isAuthenticated, error } = useAuth();
  const [processing, setProcessing] = useState(true);

  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    const code = params.get("code");
    const state = params.get("state");

    if (!code || !state) {
      setProcessing(false);
      return;
    }

    handleCallback(code, state).then(() => setProcessing(false));
  }, [handleCallback]);

  useEffect(() => {
    if (isAuthenticated && !processing) {
      window.location.href = "/";
    }
  }, [isAuthenticated, processing]);

  if (processing) {
    return (
      <div style={{ padding: 24, fontFamily: "system-ui, sans-serif" }}>
        <div data-testid="auth-processing">Completing authentication...</div>
      </div>
    );
  }

  if (error) {
    return (
      <div style={{ padding: 24, fontFamily: "system-ui, sans-serif" }}>
        <div data-testid="auth-error">{error}</div>
        <a href="/" style={{ marginTop: 12, display: "inline-block" }}>
          Back to home
        </a>
      </div>
    );
  }

  if (!new URLSearchParams(window.location.search).get("code")) {
    return (
      <div style={{ padding: 24, fontFamily: "system-ui, sans-serif" }}>
        <div data-testid="auth-error">Missing authorization code</div>
        <a href="/" style={{ marginTop: 12, display: "inline-block" }}>
          Back to home
        </a>
      </div>
    );
  }

  return null;
}
