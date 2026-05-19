/**
 * components/gallery/TemplateGallery.tsx
 *
 * P4.7 — Template Library
 *
 * Full-screen modal gallery with:
 *   - Category tabs (Web / Mobile / Social Media / Presentation / Wireframe)
 *   - Search input
 *   - Responsive thumbnail grid
 *   - Insert-on-click flow
 *   - Keyboard: Escape to close
 */

import React, { useCallback, useEffect, useRef } from "react";
import { useTemplateStore } from "../../stores/templateStore";
import { CATEGORIES, type TemplateData, type TemplateCategory } from "../../templates";

// ─────────────────────────────────────────────────────────────────────────────
// Style constants
// ─────────────────────────────────────────────────────────────────────────────

const OVERLAY: React.CSSProperties = {
    position: "fixed",
    inset: 0,
    zIndex: 1000,
    background: "rgba(10,10,20,0.75)",
    backdropFilter: "blur(8px)",
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
};

const MODAL: React.CSSProperties = {
    width: "min(92vw, 960px)",
    maxHeight: "88vh",
    display: "flex",
    flexDirection: "column",
    background: "#1e1e2e",
    border: "1px solid #313244",
    borderRadius: 16,
    overflow: "hidden",
    boxShadow: "0 24px 80px rgba(0,0,0,0.6)",
};

const MODAL_HEADER: React.CSSProperties = {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    padding: "20px 24px 0",
    flexShrink: 0,
};

const MODAL_TITLE: React.CSSProperties = {
    fontSize: 18,
    fontWeight: 700,
    color: "#cdd6f4",
    letterSpacing: "-0.01em",
};

const SEARCH_INPUT: React.CSSProperties = {
    padding: "7px 12px",
    background: "#181825",
    border: "1px solid #313244",
    borderRadius: 8,
    color: "#cdd6f4",
    fontSize: 13,
    fontFamily: "inherit",
    outline: "none",
    width: 220,
};

const CLOSE_BTN: React.CSSProperties = {
    background: "none",
    border: "none",
    color: "#6c7086",
    fontSize: 20,
    cursor: "pointer",
    padding: "4px 8px",
    borderRadius: 6,
    lineHeight: 1,
};

const TAB_ROW: React.CSSProperties = {
    display: "flex",
    gap: 0,
    borderBottom: "1px solid #313244",
    padding: "0 24px",
    flexShrink: 0,
    marginTop: 16,
};

const tabStyle = (active: boolean): React.CSSProperties => ({
    padding: "10px 18px",
    background: "none",
    border: "none",
    borderBottom: active ? "2px solid #89b4fa" : "2px solid transparent",
    cursor: "pointer",
    fontSize: 13,
    fontWeight: active ? 600 : 400,
    color: active ? "#89b4fa" : "#6c7086",
    transition: "color 0.12s, border-color 0.12s",
    fontFamily: "inherit",
    whiteSpace: "nowrap",
});

const GRID: React.CSSProperties = {
    flex: 1,
    overflowY: "auto",
    padding: 24,
    display: "grid",
    gridTemplateColumns: "repeat(auto-fill, minmax(260px, 1fr))",
    gap: 20,
    alignContent: "start",
};

const EMPTY: React.CSSProperties = {
    flex: 1,
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    color: "#45475a",
    fontSize: 14,
    padding: 40,
};

// ─────────────────────────────────────────────────────────────────────────────
// Template Card
// ─────────────────────────────────────────────────────────────────────────────

interface CardProps {
    template: TemplateData;
    hovered: boolean;
    onHover: () => void;
    onLeave: () => void;
    onInsert: () => void;
}

function TemplateCard({ template, hovered, onHover, onLeave, onInsert }: CardProps) {
    const card: React.CSSProperties = {
        borderRadius: 12,
        border: `1px solid ${hovered ? "#89b4fa" : "#313244"}`,
        background: hovered ? "#1a2744" : "#181825",
        overflow: "hidden",
        cursor: "pointer",
        transition: "border-color 0.15s, background 0.15s, transform 0.15s",
        transform: hovered ? "translateY(-2px)" : "none",
        display: "flex",
        flexDirection: "column",
    };

    return (
        <div
            style={card}
            onMouseEnter={onHover}
            onMouseLeave={onLeave}
            onClick={onInsert}
            title={`Insert "${template.name}"`}
        >
            {/* Thumbnail */}
            <div style={{ position: "relative", aspectRatio: "16/10", overflow: "hidden", background: "#11111b" }}>
                <img
                    src={template.thumbnailSvg}
                    alt={template.name}
                    style={{ width: "100%", height: "100%", objectFit: "cover", display: "block" }}
                    draggable={false}
                />
                {/* Category badge */}
                <div style={{
                    position: "absolute", top: 8, left: 8,
                    background: "rgba(30,30,46,0.85)",
                    color: "#89b4fa",
                    fontSize: 10,
                    fontWeight: 600,
                    padding: "2px 7px",
                    borderRadius: 4,
                    letterSpacing: "0.04em",
                    textTransform: "uppercase",
                }}>
                    {template.category}
                </div>
                {/* Insert overlay on hover */}
                {hovered && (
                    <div style={{
                        position: "absolute",
                        inset: 0,
                        display: "flex",
                        alignItems: "center",
                        justifyContent: "center",
                        background: "rgba(137,180,250,0.12)",
                    }}>
                        <div style={{
                            background: "#89b4fa",
                            color: "#1e1e2e",
                            fontWeight: 700,
                            fontSize: 13,
                            padding: "8px 20px",
                            borderRadius: 8,
                        }}>
                            Insert
                        </div>
                    </div>
                )}
            </div>

            {/* Metadata */}
            <div style={{ padding: "12px 14px" }}>
                <div style={{ fontSize: 13, fontWeight: 600, color: "#cdd6f4", marginBottom: 4 }}>
                    {template.name}
                </div>
                <div style={{ fontSize: 11, color: "#6c7086", lineHeight: 1.4 }}>
                    {template.description}
                </div>
                <div style={{ marginTop: 8, fontSize: 10, color: "#45475a" }}>
                    {template.shapes.length} shapes
                </div>
            </div>
        </div>
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Toast notification
// ─────────────────────────────────────────────────────────────────────────────

interface ToastProps { message: string }
function InsertToast({ message }: ToastProps) {
    return (
        <div style={{
            position: "fixed",
            bottom: 32,
            left: "50%",
            transform: "translateX(-50%)",
            zIndex: 2000,
            background: "#313244",
            color: "#cdd6f4",
            padding: "10px 20px",
            borderRadius: 10,
            fontSize: 13,
            fontWeight: 500,
            border: "1px solid #45475a",
            boxShadow: "0 4px 20px rgba(0,0,0,0.4)",
            whiteSpace: "nowrap",
        }}>
            ✦ <strong>{message}</strong> inserted on canvas
        </div>
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Main gallery
// ─────────────────────────────────────────────────────────────────────────────

export function TemplateGallery() {
    const galleryOpen = useTemplateStore((s) => s.galleryOpen);
    const activeCategory = useTemplateStore((s) => s.activeCategory);
    const searchQuery = useTemplateStore((s) => s.searchQuery);
    const hoveredId = useTemplateStore((s) => s.hoveredId);
    const lastInserted = useTemplateStore((s) => s.lastInserted);
    const { closeGallery, setCategory, setSearchQuery, setHovered, insertTemplate, visibleTemplates } = useTemplateStore();

    const visible = visibleTemplates();
    const searchRef = useRef<HTMLInputElement>(null);

    // Focus search on open
    useEffect(() => {
        if (galleryOpen) setTimeout(() => searchRef.current?.focus(), 60);
    }, [galleryOpen]);

    // Escape to close
    useEffect(() => {
        if (!galleryOpen) return;
        const handler = (e: KeyboardEvent) => { if (e.key === "Escape") closeGallery(); };
        window.addEventListener("keydown", handler);
        return () => window.removeEventListener("keydown", handler);
    }, [galleryOpen, closeGallery]);

    const handleInsert = useCallback(
        (id: string) => {
            // Offset so the template doesn't land exactly at 0,0
            insertTemplate(id, 120, 120);
        },
        [insertTemplate]
    );

    // Stop modal click from propagating to overlay
    const stopPropagation = (e: React.MouseEvent) => e.stopPropagation();

    if (!galleryOpen) {
        return lastInserted ? <InsertToast message={lastInserted} /> : null;
    }

    return (
        <>
            {/* Overlay backdrop */}
            <div style={OVERLAY} onClick={closeGallery}>
                <div style={MODAL} onClick={stopPropagation}>

                    {/* Header */}
                    <div style={MODAL_HEADER}>
                        <div style={MODAL_TITLE}>Template Library</div>
                        <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
                            <input
                                ref={searchRef}
                                style={SEARCH_INPUT}
                                placeholder="Search templates…"
                                value={searchQuery}
                                onChange={(e) => setSearchQuery(e.target.value)}
                            />
                            <button style={CLOSE_BTN} onClick={closeGallery} title="Close (Esc)">
                                ✕
                            </button>
                        </div>
                    </div>

                    {/* Category tabs */}
                    <div style={TAB_ROW}>
                        {CATEGORIES.map((cat) => (
                            <button
                                key={cat}
                                style={tabStyle(activeCategory === cat)}
                                onClick={() => setCategory(cat as TemplateCategory)}
                            >
                                {cat}
                            </button>
                        ))}
                    </div>

                    {/* Grid or empty state */}
                    {visible.length === 0 ? (
                        <div style={EMPTY}>
                            {searchQuery
                                ? `No templates match "${searchQuery}" in ${activeCategory}.`
                                : `No templates in ${activeCategory} yet — contributions welcome!`}
                        </div>
                    ) : (
                        <div style={GRID}>
                            {visible.map((tpl) => (
                                <TemplateCard
                                    key={tpl.id}
                                    template={tpl}
                                    hovered={hoveredId === tpl.id}
                                    onHover={() => setHovered(tpl.id)}
                                    onLeave={() => setHovered(null)}
                                    onInsert={() => handleInsert(tpl.id)}
                                />
                            ))}
                        </div>
                    )}

                    {/* Footer */}
                    <div style={{
                        padding: "12px 24px",
                        borderTop: "1px solid #313244",
                        display: "flex",
                        justifyContent: "space-between",
                        alignItems: "center",
                        flexShrink: 0,
                    }}>
                        <span style={{ fontSize: 11, color: "#45475a" }}>
                            {visible.length} template{visible.length !== 1 ? "s" : ""} ·{" "}
                            Click to insert · Templates are fully editable after insertion
                        </span>
                        <a
                            href="https://github.com/navidrezadoost/Logos/blob/main/CONTRIBUTING.md"
                            target="_blank"
                            rel="noopener noreferrer"
                            style={{ fontSize: 11, color: "#6c7086", textDecoration: "none" }}
                        >
                            Contribute a template →
                        </a>
                    </div>
                </div>
            </div>

            {/* Toast (outside modal so it persists after modal closes) */}
            {lastInserted && <InsertToast message={lastInserted} />}
        </>
    );
}
