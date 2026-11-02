export interface Profile {
  id: string;
  fullname: string;
  email: string;
  "default-team-id"?: string;
  "default-project-id"?: string;
}

export interface Project {
  id: string;
  "team-id": string;
  name: string;
  description?: string;
  "photo-id"?: string;
  "file-id"?: string;
  "is-default"?: boolean;
  "created-at"?: string;
  "modified-at"?: string;
}

export interface CreatedProject extends Project {
  pages: string[];
}

export interface FileSummary {
  id: string;
  "project-id": string;
  name: string;
}

export interface FileDetail {
  id: string;
  "project-id": string;
  name: string;
  data: {
    pages?: string[];
    [key: string]: unknown;
  };
}
