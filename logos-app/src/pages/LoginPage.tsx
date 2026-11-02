import React, { useState } from "react";
import { login, prepareRegister, register } from "../api/client";
import { theme } from "../theme/colors";

interface LoginPageProps {
  onAuthenticated: () => void;
}

export function LoginPage({ onAuthenticated }: LoginPageProps): React.ReactElement {
  const [mode, setMode] = useState<"login" | "register">("login");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [fullName, setFullName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    setBusy(true);
    try {
      if (mode === "login") {
        await login(email, password);
      } else {
        const { token } = await prepareRegister(email, password, fullName);
        await register(token);
      }
      onAuthenticated();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Authentication failed");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div
      style={{
        minHeight: "100vh",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        background: theme.appBg,
        color: theme.text,
        fontFamily: "'Inter', system-ui, sans-serif",
      }}
    >
      <form
        onSubmit={handleSubmit}
        style={{
          width: 360,
          padding: 32,
          background: theme.panel,
          borderRadius: 12,
          boxShadow: "0 8px 32px rgba(0,0,0,0.35)",
        }}
      >
        <h1 style={{ margin: "0 0 8px", fontSize: 22, fontWeight: 700 }}>Logos</h1>
        <p style={{ margin: "0 0 24px", color: "#a6adc8", fontSize: 14 }}>
          {mode === "login" ? "Sign in to your projects" : "Create your account"}
        </p>

        {mode === "register" && (
          <label style={labelStyle}>
            Full name
            <input
              value={fullName}
              onChange={(e) => setFullName(e.target.value)}
              required
              style={inputStyle}
            />
          </label>
        )}

        <label style={labelStyle}>
          Email
          <input
            type="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            required
            autoComplete="email"
            style={inputStyle}
          />
        </label>

        <label style={labelStyle}>
          Password
          <input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            required
            autoComplete={mode === "login" ? "current-password" : "new-password"}
            style={inputStyle}
          />
        </label>

        {error && (
          <p style={{ color: "#f38ba8", fontSize: 13, margin: "0 0 12px" }}>{error}</p>
        )}

        <button type="submit" disabled={busy} style={primaryBtn}>
          {busy ? "Please wait…" : mode === "login" ? "Sign in" : "Create account"}
        </button>

        <button
          type="button"
          onClick={() => {
            setMode(mode === "login" ? "register" : "login");
            setError(null);
          }}
          style={linkBtn}
        >
          {mode === "login" ? "Need an account? Register" : "Already have an account? Sign in"}
        </button>
      </form>
    </div>
  );
}

const labelStyle: React.CSSProperties = {
  display: "block",
  fontSize: 13,
  fontWeight: 500,
  marginBottom: 16,
  color: "#bac2de",
};

const inputStyle: React.CSSProperties = {
  display: "block",
  width: "100%",
  marginTop: 6,
  padding: "10px 12px",
  borderRadius: 8,
  border: `1px solid ${theme.borderStrong}`,
  background: theme.surface,
  color: theme.text,
  fontSize: 14,
  boxSizing: "border-box",
};

const primaryBtn: React.CSSProperties = {
  width: "100%",
  padding: "11px 16px",
  borderRadius: 8,
  border: "none",
  background: theme.accent,
  color: theme.onAccent,
  fontWeight: 600,
  fontSize: 14,
  cursor: "pointer",
  marginBottom: 12,
};

const linkBtn: React.CSSProperties = {
  width: "100%",
  padding: 8,
  border: "none",
  background: "transparent",
  color: theme.accent,
  fontSize: 13,
  cursor: "pointer",
};
