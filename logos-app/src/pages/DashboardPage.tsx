import React, { useCallback, useEffect, useState } from "react";
import {
  createProject,
  getProfile,
  getProjects,
  openPenpotWorkspace,
  projectPhotoUrl,
  resolveProjectDesign,
} from "../api/client";
import type { Profile, Project } from "../api/types";
import { CreateProjectModal } from "../components/dashboard/CreateProjectModal";
import { theme } from "../theme/colors";

export function DashboardPage(): React.ReactElement {
  const [profile, setProfile] = useState<Profile | null>(null);
  const [projects, setProjects] = useState<Project[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [modalOpen, setModalOpen] = useState(false);
  const [creating, setCreating] = useState(false);
  const [openingId, setOpeningId] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const p = await getProfile();
      setProfile(p);
      const teamId = p["default-team-id"];
      if (!teamId) {
        setProjects([]);
        return;
      }
      const list = await getProjects(teamId);
      setProjects(list.filter((proj) => !proj["is-default"]));
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load projects");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  async function handleCreate(params: {
    name: string;
    description: string;
    photo: File | null;
  }) {
    const teamId = profile?.["default-team-id"];
    if (!teamId) return;

    setCreating(true);
    setError(null);
    try {
      const created = await createProject({
        teamId,
        name: params.name,
        description: params.description,
        photo: params.photo,
      });
      setModalOpen(false);
      const fileId = created["file-id"];
      const pageId = created.pages?.[0];
      if (fileId && pageId) {
        openPenpotWorkspace(teamId, fileId, pageId);
        return;
      }
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to create project");
    } finally {
      setCreating(false);
    }
  }

  async function openProject(project: Project) {
    setOpeningId(project.id);
    setError(null);
    try {
      const teamId = profile?.["default-team-id"];
      if (!teamId) {
        throw new Error("No team found for your account.");
      }
      const { fileId, pageId } = await resolveProjectDesign(project);
      openPenpotWorkspace(teamId, fileId, pageId);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to open project");
    } finally {
      setOpeningId(null);
    }
  }

  return (
    <div style={page}>
      <header style={header}>
        <div>
          <h1 style={{ margin: 0, fontSize: 22, fontWeight: 700 }}>Projects</h1>
          {profile && (
            <p style={{ margin: "4px 0 0", color: "#a6adc8", fontSize: 13 }}>
              {profile.fullname || profile.email}
            </p>
          )}
        </div>
        <button type="button" style={newBtn} onClick={() => setModalOpen(true)}>
          + New project
        </button>
      </header>

      {error && <p style={errorBanner}>{error}</p>}

      {loading ? (
        <p style={{ color: "#a6adc8" }}>Loading projects…</p>
      ) : projects.length === 0 ? (
        <div style={empty}>
          <p style={{ margin: 0, fontSize: 16, fontWeight: 600 }}>No projects yet</p>
          <p style={{ margin: "8px 0 20px", color: "#a6adc8", fontSize: 14 }}>
            Create a project with a title and optional cover — then jump straight into design.
          </p>
          <button type="button" style={newBtn} onClick={() => setModalOpen(true)}>
            Create your first project
          </button>
        </div>
      ) : (
        <div style={grid}>
          {projects.map((project) => (
            <button
              key={project.id}
              type="button"
              style={card}
              disabled={openingId === project.id}
              onClick={() => void openProject(project)}
            >
              <div style={thumb}>
                {projectPhotoUrl(project["photo-id"]) ? (
                  <img
                    src={projectPhotoUrl(project["photo-id"])!}
                    alt=""
                    style={{ width: "100%", height: "100%", objectFit: "cover" }}
                  />
                ) : (
                  <span style={{ color: "#585b70", fontSize: 28 }}>◫</span>
                )}
              </div>
              <div style={cardBody}>
                <div style={{ fontWeight: 600, fontSize: 14 }}>{project.name}</div>
                {project.description && (
                  <div
                    style={{
                      marginTop: 4,
                      color: "#a6adc8",
                      fontSize: 12,
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                      whiteSpace: "nowrap",
                    }}
                  >
                    {project.description}
                  </div>
                )}
              </div>
            </button>
          ))}
        </div>
      )}

      <CreateProjectModal
        open={modalOpen}
        busy={creating}
        onClose={() => setModalOpen(false)}
        onCreate={(params) => void handleCreate(params)}
      />
    </div>
  );
}

const page: React.CSSProperties = {
  minHeight: "100vh",
  background: theme.appBg,
  color: theme.text,
  fontFamily: "'Inter', system-ui, sans-serif",
  padding: "32px 40px",
};

const header: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  justifyContent: "space-between",
  marginBottom: 32,
};

const newBtn: React.CSSProperties = {
  padding: "10px 18px",
  borderRadius: 8,
  border: "none",
  background: theme.accent,
  color: theme.onAccent,
  fontWeight: 600,
  fontSize: 14,
  cursor: "pointer",
};

const errorBanner: React.CSSProperties = {
  color: "#f38ba8",
  background: "rgba(243,139,168,0.1)",
  padding: "10px 14px",
  borderRadius: 8,
  fontSize: 13,
  marginBottom: 20,
};

const empty: React.CSSProperties = {
  textAlign: "center",
  padding: "80px 24px",
  background: theme.panel,
  borderRadius: 12,
  maxWidth: 480,
  margin: "40px auto",
};

const grid: React.CSSProperties = {
  display: "grid",
  gridTemplateColumns: "repeat(auto-fill, minmax(220px, 1fr))",
  gap: 20,
};

const card: React.CSSProperties = {
  padding: 0,
  border: `1px solid ${theme.border}`,
  borderRadius: 10,
  background: theme.panel,
  cursor: "pointer",
  textAlign: "left",
  color: theme.text,
  overflow: "hidden",
  transition: "border-color 0.15s",
};

const thumb: React.CSSProperties = {
  height: 140,
  background: theme.surface,
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
};

const cardBody: React.CSSProperties = {
  padding: "12px 14px",
};
