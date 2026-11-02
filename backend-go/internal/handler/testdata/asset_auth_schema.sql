-- Minimal disposable PostgreSQL schema for TestAssetByIDHandlerEnforcesResourceAuthorization.
-- This is a test harness, not a production migration or a complete Logos schema.
CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE profile (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    email text NOT NULL,
    fullname text,
    lang text,
    theme text,
    photo_id uuid,
    deleted_at timestamptz
);

CREATE TABLE team (
    id uuid PRIMARY KEY,
    name text NOT NULL,
    is_default boolean NOT NULL DEFAULT false,
    deleted_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    modified_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE team_profile_rel (
    team_id uuid NOT NULL,
    profile_id uuid NOT NULL,
    is_owner boolean NOT NULL DEFAULT false,
    is_admin boolean NOT NULL DEFAULT false,
    can_edit boolean NOT NULL DEFAULT false
);

CREATE TABLE project (
    id uuid PRIMARY KEY,
    team_id uuid NOT NULL,
    name text NOT NULL,
    is_default boolean NOT NULL DEFAULT false,
    photo_id uuid,
    created_at timestamptz NOT NULL DEFAULT now(),
    modified_at timestamptz NOT NULL DEFAULT now(),
    deleted_at timestamptz
);

CREATE TABLE project_profile_rel (
    project_id uuid NOT NULL,
    profile_id uuid NOT NULL,
    is_owner boolean NOT NULL DEFAULT false,
    is_admin boolean NOT NULL DEFAULT false,
    can_edit boolean NOT NULL DEFAULT false
);

CREATE TABLE team_project_profile_rel (
    team_id uuid NOT NULL,
    project_id uuid NOT NULL,
    profile_id uuid NOT NULL,
    is_pinned boolean NOT NULL DEFAULT false,
    UNIQUE (team_id, project_id, profile_id)
);

CREATE TABLE file (
    id uuid PRIMARY KEY,
    project_id uuid NOT NULL,
    name text NOT NULL,
    is_shared boolean NOT NULL DEFAULT false,
    features text[] NOT NULL DEFAULT '{}',
    data jsonb,
    version integer NOT NULL DEFAULT 0,
    revn integer NOT NULL DEFAULT 0,
    vern integer NOT NULL DEFAULT 0,
    comment_thread_seqn integer NOT NULL DEFAULT 0,
    created_at timestamptz NOT NULL DEFAULT now(),
    modified_at timestamptz NOT NULL DEFAULT now(),
    deleted_at timestamptz
);

CREATE TABLE file_profile_rel (
    file_id uuid NOT NULL,
    profile_id uuid NOT NULL,
    is_owner boolean NOT NULL DEFAULT false,
    is_admin boolean NOT NULL DEFAULT false,
    can_edit boolean NOT NULL DEFAULT false
);

CREATE TABLE storage_object (
    id uuid PRIMARY KEY,
    backend text NOT NULL,
    size bigint NOT NULL,
    metadata jsonb NOT NULL,
    deleted_at timestamptz
);

CREATE TABLE file_media_object (
    id uuid PRIMARY KEY,
    file_id uuid NOT NULL,
    is_local boolean NOT NULL,
    name text NOT NULL,
    media_id uuid NOT NULL,
    width integer NOT NULL,
    height integer NOT NULL,
    mtype text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    deleted_at timestamptz
);

CREATE TABLE file_tagged_object_thumbnail (
    file_id uuid NOT NULL,
    tag text NOT NULL,
    object_id text NOT NULL,
    media_id uuid NOT NULL,
    updated_at timestamptz NOT NULL,
    deleted_at timestamptz
);

CREATE TABLE file_thumbnail (
    file_id uuid NOT NULL,
    revn integer NOT NULL,
    media_id uuid NOT NULL,
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL,
    deleted_at timestamptz
);

CREATE TABLE team_font_variant (
    id uuid PRIMARY KEY,
    team_id uuid NOT NULL,
    profile_id uuid NOT NULL,
    font_id uuid NOT NULL,
    font_family text NOT NULL,
    font_weight integer NOT NULL,
    font_style text NOT NULL,
    woff1_file_id uuid,
    woff2_file_id uuid,
    otf_file_id uuid,
    ttf_file_id uuid,
    deleted_at timestamptz
);

CREATE TABLE share_link (
    id uuid PRIMARY KEY,
    file_id uuid NOT NULL,
    owner_id uuid,
    who_comment text NOT NULL,
    who_inspect text NOT NULL,
    pages uuid[] NOT NULL DEFAULT '{}',
    created_at timestamptz NOT NULL DEFAULT now()
);
