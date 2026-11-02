import type {
  CreatedProject,
  FileDetail,
  FileSummary,
  Profile,
  Project,
} from "./types";

const API = "/api/rpc/command";

async function apiFetch<T>(
  path: string,
  init: RequestInit = {}
): Promise<T> {
  const res = await fetch(`${API}${path}`, {
    credentials: "include",
    ...init,
    headers: {
      ...(init.body instanceof FormData ? {} : { "Content-Type": "application/json" }),
      ...init.headers,
    },
  });

  if (!res.ok) {
    let message = res.statusText;
    try {
      const err = (await res.json()) as { hint?: string; code?: string };
      message = err.hint ?? err.code ?? message;
    } catch {
      /* ignore */
    }
    throw new Error(message);
  }

  if (res.status === 204) {
    return undefined as T;
  }
  return (await res.json()) as T;
}

export async function getProfile(): Promise<Profile> {
  return apiFetch<Profile>("/get-profile", { method: "POST", body: "{}" });
}

export async function login(email: string, password: string): Promise<Profile> {
  return apiFetch<Profile>("/login-with-password", {
    method: "POST",
    body: JSON.stringify({ email, password }),
  });
}

export async function prepareRegister(
  email: string,
  password: string,
  fullName: string
): Promise<{ token: string }> {
  return apiFetch<{ token: string }>("/prepare-register-profile", {
    method: "POST",
    body: JSON.stringify({ email, password, fullname: fullName }),
  });
}

export async function register(token: string): Promise<Profile> {
  return apiFetch<Profile>("/register-profile", {
    method: "POST",
    body: JSON.stringify({ token }),
  });
}

export async function getProjects(teamId: string): Promise<Project[]> {
  return apiFetch<Project[]>(`/get-projects?team-id=${encodeURIComponent(teamId)}`);
}

export async function getProjectFiles(projectId: string): Promise<FileSummary[]> {
  return apiFetch<FileSummary[]>(
    `/get-project-files?project-id=${encodeURIComponent(projectId)}`
  );
}

export async function getFile(fileId: string): Promise<FileDetail> {
  return apiFetch<FileDetail>(`/get-file?id=${encodeURIComponent(fileId)}`);
}

export interface CreateProjectParams {
  teamId: string;
  name: string;
  description?: string;
  photo?: File | null;
}

export async function createProject(
  params: CreateProjectParams
): Promise<CreatedProject> {
  if (params.photo) {
    const form = new FormData();
    form.set("team-id", params.teamId);
    form.set("name", params.name);
    if (params.description?.trim()) {
      form.set("description", params.description.trim());
    }
    form.set("photo", params.photo);
    return apiFetch<CreatedProject>("/create-project", {
      method: "POST",
      body: form,
    });
  }

  return apiFetch<CreatedProject>("/create-project", {
    method: "POST",
    body: JSON.stringify({
      "team-id": params.teamId,
      name: params.name,
      description: params.description?.trim() ?? "",
    }),
  });
}

export function projectPhotoUrl(photoId: string | undefined): string | null {
  if (!photoId) return null;
  return `/assets/by-id/${photoId}`;
}

export function workspaceURL(
  teamId: string,
  fileId: string,
  pageId: string
): string {
  const params = new URLSearchParams({
    "team-id": teamId,
    "file-id": fileId,
    "page-id": pageId,
  });
  return `/workspace.html#/workspace?${params.toString()}`;
}

/** Navigate into the full Loogs design editor. */
export function openWorkspace(
  teamId: string,
  fileId: string,
  pageId: string
): void {
  window.location.assign(workspaceURL(teamId, fileId, pageId));
}

/** @deprecated Use workspaceURL. */
export function workspacePath(
  teamId: string,
  fileId: string,
  pageId: string
): string {
  return workspaceURL(teamId, fileId, pageId);
}

/** Resolve file + first page for opening a project in the editor. */
export async function resolveProjectDesign(
  project: Project
): Promise<{ fileId: string; pageId: string }> {
  let fileId = project["file-id"];
  if (!fileId) {
    const files = await getProjectFiles(project.id);
    fileId = files[0]?.id;
  }
  if (!fileId) {
    throw new Error("This project has no design file yet.");
  }

  const file = await getFile(fileId);
  const pages = file.data?.pages;
  const pageId = Array.isArray(pages) && pages.length > 0 ? String(pages[0]) : "";
  if (!pageId) {
    throw new Error("Could not load the design page.");
  }
  return { fileId, pageId };
}
