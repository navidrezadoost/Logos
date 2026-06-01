-- Project cover image and description for Figma-style project creation.
ALTER TABLE project ADD COLUMN IF NOT EXISTS description text NOT NULL DEFAULT '';
ALTER TABLE project ADD COLUMN IF NOT EXISTS photo_id uuid REFERENCES storage_object(id);
