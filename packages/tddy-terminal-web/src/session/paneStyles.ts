import type React from "react";

export const affordanceBtnStyle: React.CSSProperties = {
  position: "absolute",
  top: 4,
  right: 8,
  zIndex: 5,
  fontSize: 11,
  padding: "2px 8px",
  background: "var(--muted, #2a2b3d)",
  color: "var(--muted-foreground, #a9b1d6)",
  border: "1px solid var(--border, #333)",
  borderRadius: 4,
  cursor: "pointer",
};

export const paneCoverStyle: React.CSSProperties = {
  position: "absolute",
  inset: 0,
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  padding: 16,
  backgroundColor: "rgba(0,0,0,0.55)",
  color: "#e0e0e0",
  fontSize: 14,
  textAlign: "center",
  pointerEvents: "auto",
};
