import React, { useRef, useState } from "react";
import { theme } from "../../theme/colors";

interface CreateProjectModalProps {
  open: boolean;
  busy: boolean;
  onClose: () => void;
  onCreate: (params: {
    name: string;
    description: string;
    photo: File | null;
  }) => void;
}

export function CreateProjectModal({
  open,
  busy,
  onClose,
  onCreate,
}: CreateProjectModalProps): React.ReactElement | null {
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [photo, setPhoto] = useState<File | null>(null);
  const [preview, setPreview] = useState<string | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  if (!open) return null;

  function reset() {
    setName("");
    setDescription("");
    setPhoto(null);
    setPreview(null);
    if (fileRef.current) fileRef.current.value = "";
  }

  function handlePhotoChange(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0] ?? null;
    setPhoto(file);
    if (preview) URL.revokeObjectURL(preview);
    setPreview(file ? URL.createObjectURL(file) : null);
  }

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!name.trim()) return;
    onCreate({ name: name.trim(), description, photo });
  }

  function handleClose() {
    reset();
    onClose();
  }

  return (
    <div style={overlay} onClick={handleClose}>
      <div style={dialog} onClick={(e) => e.stopPropagation()}>
        <h2 style={{ margin: "0 0 4px", fontSize: 18 }}>New project</h2>
        <p style={{ margin: "0 0 20px", color: "#a6adc8", fontSize: 13 }}>
          Give your project a name. You&apos;ll go straight into the design canvas.
        </p>

        <form onSubmit={handleSubmit}>
          <label style={label}>
            Title <span style={{ color: "#f38ba8" }}>*</span>
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="Mobile app redesign"
              required
              autoFocus
              style={input}
            />
          </label>

          <label style={label}>
            Description <span style={{ color: "#6c7086", fontWeight: 400 }}>(optional)</span>
            <textarea
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder="What is this project about?"
              rows={3}
              style={{ ...input, resize: "vertical" }}
            />
          </label>

          <label style={label}>
            Cover photo <span style={{ color: "#6c7086", fontWeight: 400 }}>(optional)</span>
            <div
              style={photoBox}
              onClick={() => fileRef.current?.click()}
              role="button"
              tabIndex={0}
              onKeyDown={(e) => e.key === "Enter" && fileRef.current?.click()}
            >
              {preview ? (
                <img src={preview} alt="" style={{ width: "100%", height: "100%", objectFit: "cover" }} />
              ) : (
                <span style={{ color: "#6c7086", fontSize: 13 }}>Click to upload an image</span>
              )}
            </div>
            <input
              ref={fileRef}
              type="file"
              accept="image/*"
              onChange={handlePhotoChange}
              style={{ display: "none" }}
            />
          </label>

          <div style={{ display: "flex", gap: 10, justifyContent: "flex-end", marginTop: 24 }}>
            <button type="button" onClick={handleClose} style={secondaryBtn} disabled={busy}>
              Cancel
            </button>
            <button type="submit" style={primaryBtn} disabled={busy || !name.trim()}>
              {busy ? "Creating…" : "Create project"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

const overlay: React.CSSProperties = {
  position: "fixed",
  inset: 0,
  background: "rgba(0,0,0,0.55)",
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  zIndex: 1000,
};

const dialog: React.CSSProperties = {
  width: 420,
  maxWidth: "92vw",
  padding: 28,
  background: theme.panel,
  borderRadius: 12,
  color: theme.text,
  fontFamily: "'Inter', system-ui, sans-serif",
  boxShadow: "0 16px 48px rgba(0,0,0,0.45)",
};

const label: React.CSSProperties = {
  display: "block",
  fontSize: 13,
  fontWeight: 600,
  marginBottom: 14,
  color: "#bac2de",
};

const input: React.CSSProperties = {
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

const photoBox: React.CSSProperties = {
  marginTop: 6,
  height: 120,
  borderRadius: 8,
  border: `1px dashed ${theme.borderStrong}`,
  background: theme.surface,
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  overflow: "hidden",
  cursor: "pointer",
};

const primaryBtn: React.CSSProperties = {
  padding: "10px 18px",
  borderRadius: 8,
  border: "none",
  background: theme.accent,
  color: theme.onAccent,
  fontWeight: 600,
  fontSize: 14,
  cursor: "pointer",
};

const secondaryBtn: React.CSSProperties = {
  padding: "10px 18px",
  borderRadius: 8,
  border: "1px solid #45475a",
  background: "transparent",
  color: "#cdd6f4",
  fontSize: 14,
  cursor: "pointer",
};
