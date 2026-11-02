import React, { useCallback, useEffect, useState } from "react";
import { Navigate, Route, Routes, useNavigate } from "react-router-dom";
import { getProfile } from "./api/client";
import { LegacyHashRedirect } from "./LegacyHashRedirect";
import { WorkspaceRedirect } from "./WorkspaceRedirect";
import { DashboardPage } from "./pages/DashboardPage";
import { LoginPage } from "./pages/LoginPage";
import { theme } from "./theme/colors";

export default function App(): React.ReactElement {
  const [authChecked, setAuthChecked] = useState(false);
  const [authenticated, setAuthenticated] = useState(false);
  const navigate = useNavigate();

  const checkAuth = useCallback(async () => {
    try {
      const profile = await getProfile();
      const isAuthed =
        profile.id !== "00000000-0000-0000-0000-000000000000" && Boolean(profile.email);
      setAuthenticated(isAuthed);
    } catch {
      setAuthenticated(false);
    } finally {
      setAuthChecked(true);
    }
  }, []);

  useEffect(() => {
    void checkAuth();
  }, [checkAuth]);

  if (!authChecked) {
    return (
      <div
        style={{
          minHeight: "100vh",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          background: theme.appBg,
          color: theme.textSecondary,
          fontFamily: "'Inter', system-ui, sans-serif",
        }}
      >
        Loading…
      </div>
    );
  }

  return (
    <>
      <LegacyHashRedirect />
      <Routes>
      <Route
        path="/login"
        element={
          authenticated ? (
            <Navigate to="/" replace />
          ) : (
            <LoginPage
              onAuthenticated={() => {
                setAuthenticated(true);
                navigate("/", { replace: true });
              }}
            />
          )
        }
      />
      <Route
        path="/"
        element={
          authenticated ? <DashboardPage /> : <Navigate to="/login" replace />
        }
      />
      <Route
        path="/workspace/:projectId/:fileId"
        element={<WorkspaceRedirect />}
      />
      <Route path="*" element={<Navigate to="/" replace />} />
    </Routes>
    </>
  );
}
