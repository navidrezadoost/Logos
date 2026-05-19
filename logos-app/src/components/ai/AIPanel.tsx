/**
 * components/ai/AIPanel.tsx
 *
 * AI Design Assistant panel — three tabs:
 *   • Generate Layout  — natural language → shapes on canvas
 *   • Apply Palette    — semantic description → fill colors
 *   • Breakpoints      — duplicate frame at multiple widths
 *
 * Message history is rendered below the controls.
 */

import React, { useRef, useEffect, useState, useCallback } from "react";
import { useAiStore, type AiTab, type AiMessage } from "../../stores/aiStore";

// ─────────────────────────────────────────────────────────────────────────────
// Styles (CSS-in-JS objects)
// ─────────────────────────────────────────────────────────────────────────────

const PANEL: React.CSSProperties = {
    width: 300,
    minWidth: 300,
    height: "100%",
    display: "flex",
    flexDirection: "column",
    background: "#181825",
    borderLeft: "1px solid #313244",
    fontFamily: "'Inter', system-ui, sans-serif",
    fontSize: 13,
    color: "#cdd6f4",
    overflow: "hidden",
};

const HEADER: React.CSSProperties = {
    padding: "12px 16px 0",
    borderBottom: "1px solid #313244",
    flexShrink: 0,
};

const TITLE: React.CSSProperties = {
    fontSize: 13,
    fontWeight: 600,
    color: "#a6adc8",
    letterSpacing: "0.05em",
    textTransform: "uppercase",
    marginBottom: 10,
};

const TABS: React.CSSProperties = {
    display: "flex",
    gap: 0,
};

const TAB_BASE: React.CSSProperties = {
    flex: 1,
    padding: "6px 4px",
    background: "none",
    border: "none",
    borderBottom: "2px solid transparent",
    cursor: "pointer",
    fontSize: 12,
    fontWeight: 500,
    color: "#6c7086",
    transition: "color 0.15s, border-color 0.15s",
};

const TAB_ACTIVE: React.CSSProperties = {
    ...TAB_BASE,
    color: "#89b4fa",
    borderBottomColor: "#89b4fa",
};

const BODY: React.CSSProperties = {
    flex: 1,
    display: "flex",
    flexDirection: "column",
    overflow: "hidden",
};

const CONTROLS: React.CSSProperties = {
    padding: "12px 16px",
    display: "flex",
    flexDirection: "column",
    gap: 8,
    borderBottom: "1px solid #313244",
    flexShrink: 0,
};

const LABEL: React.CSSProperties = {
    fontSize: 11,
    color: "#6c7086",
    marginBottom: 2,
};

const TEXTAREA: React.CSSProperties = {
    width: "100%",
    minHeight: 64,
    padding: "8px 10px",
    background: "#1e1e2e",
    border: "1px solid #313244",
    borderRadius: 6,
    color: "#cdd6f4",
    fontSize: 13,
    fontFamily: "inherit",
    resize: "vertical",
    outline: "none",
    boxSizing: "border-box",
};

const INPUT: React.CSSProperties = {
    width: "100%",
    padding: "6px 10px",
    background: "#1e1e2e",
    border: "1px solid #313244",
    borderRadius: 6,
    color: "#cdd6f4",
    fontSize: 13,
    fontFamily: "inherit",
    outline: "none",
    boxSizing: "border-box",
};

const BTN_PRIMARY: React.CSSProperties = {
    padding: "7px 14px",
    background: "#89b4fa",
    color: "#1e1e2e",
    border: "none",
    borderRadius: 6,
    fontSize: 13,
    fontWeight: 600,
    cursor: "pointer",
    alignSelf: "flex-end",
    minWidth: 80,
};

const BTN_GHOST: React.CSSProperties = {
    padding: "4px 8px",
    background: "transparent",
    color: "#6c7086",
    border: "1px solid #313244",
    borderRadius: 4,
    fontSize: 11,
    cursor: "pointer",
};

const HISTORY: React.CSSProperties = {
    flex: 1,
    overflowY: "auto",
    padding: "12px 16px",
    display: "flex",
    flexDirection: "column",
    gap: 8,
};

// ─────────────────────────────────────────────────────────────────────────────
// Message bubble
// ─────────────────────────────────────────────────────────────────────────────

function MessageBubble({ msg }: { msg: AiMessage }) {
    const isUser = msg.role === "user";
    const isError = msg.role === "error";

    const bubbleStyle: React.CSSProperties = {
        maxWidth: "90%",
        padding: "8px 12px",
        borderRadius: 8,
        fontSize: 12,
        lineHeight: 1.5,
        wordBreak: "break-word",
        alignSelf: isUser ? "flex-end" : "flex-start",
        background: isError ? "#3b1a1a" : isUser ? "#1a2744" : "#1e1e2e",
        border: `1px solid ${isError ? "#f38ba8" : isUser ? "#45475a" : "#313244"}`,
        color: isError ? "#f38ba8" : "#cdd6f4",
    };

    return (
        <div style={bubbleStyle}>
            <span style={{ fontWeight: 600, color: isUser ? "#89b4fa" : "#a6adc8", fontSize: 10, display: "block", marginBottom: 2 }}>
                {isUser ? "You" : isError ? "Error" : "AI"}
            </span>
            {msg.content}
            {msg.createdShapeIds && msg.createdShapeIds.length > 0 && (
                <div style={{ marginTop: 4, fontSize: 10, color: "#6c7086" }}>
                    {msg.createdShapeIds.length} shape(s) created
                </div>
            )}
        </div>
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Generate Layout tab
// ─────────────────────────────────────────────────────────────────────────────

function GenerateTab() {
    const [prompt, setPrompt] = useState("");
    const [x, setX] = useState("0");
    const [y, setY] = useState("0");
    const [width, setWidth] = useState("800");
    const sendGeneratePrompt = useAiStore((s) => s.sendGeneratePrompt);
    const isLoading = useAiStore((s) => s.isLoading);

    const handleSubmit = useCallback(() => {
        if (!prompt.trim() || isLoading) return;
        sendGeneratePrompt(prompt.trim(), Number(x) || 0, Number(y) || 0, Number(width) || 800);
        setPrompt("");
    }, [prompt, x, y, width, isLoading, sendGeneratePrompt]);

    const handleKeyDown = (e: React.KeyboardEvent) => {
        if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) handleSubmit();
    };

    return (
        <div style={CONTROLS}>
            <div>
                <div style={LABEL}>Describe the layout you want</div>
                <textarea
                    style={TEXTAREA}
                    value={prompt}
                    onChange={(e) => setPrompt(e.target.value)}
                    onKeyDown={handleKeyDown}
                    placeholder="e.g. a hero section with heading and two buttons"
                    disabled={isLoading}
                />
            </div>
            <div style={{ display: "flex", gap: 8 }}>
                <div style={{ flex: 1 }}>
                    <div style={LABEL}>X</div>
                    <input style={INPUT} type="number" value={x} onChange={(e) => setX(e.target.value)} disabled={isLoading} />
                </div>
                <div style={{ flex: 1 }}>
                    <div style={LABEL}>Y</div>
                    <input style={INPUT} type="number" value={y} onChange={(e) => setY(e.target.value)} disabled={isLoading} />
                </div>
                <div style={{ flex: 1 }}>
                    <div style={LABEL}>Width</div>
                    <input style={INPUT} type="number" value={width} onChange={(e) => setWidth(e.target.value)} disabled={isLoading} />
                </div>
            </div>
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                <span style={{ fontSize: 10, color: "#45475a" }}>⌘↵ to send</span>
                <button style={{ ...BTN_PRIMARY, opacity: isLoading ? 0.5 : 1 }} onClick={handleSubmit} disabled={isLoading}>
                    {isLoading ? "Generating…" : "Generate"}
                </button>
            </div>
        </div>
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Apply Palette tab
// ─────────────────────────────────────────────────────────────────────────────

const PRESET_PALETTES = [
    "Dark Mode", "Warm Earth", "Ocean Blue", "Pastel", "Sunset", "Light / Clean",
];

function PaletteTab() {
    const [description, setDescription] = useState("");
    const sendPalettePrompt = useAiStore((s) => s.sendPalettePrompt);
    const isLoading = useAiStore((s) => s.isLoading);
    const lastPalette = useAiStore((s) => s.lastPalette);

    const handleSubmit = useCallback(() => {
        if (!description.trim() || isLoading) return;
        sendPalettePrompt(description.trim());
        setDescription("");
    }, [description, isLoading, sendPalettePrompt]);

    return (
        <div style={CONTROLS}>
            <div>
                <div style={LABEL}>Palette description</div>
                <textarea
                    style={{ ...TEXTAREA, minHeight: 48 }}
                    value={description}
                    onChange={(e) => setDescription(e.target.value)}
                    onKeyDown={(e) => { if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) handleSubmit(); }}
                    placeholder="e.g. dark mode, ocean blue, warm earth"
                    disabled={isLoading}
                />
            </div>

            {/* Quick-select presets */}
            <div style={{ display: "flex", flexWrap: "wrap", gap: 4 }}>
                {PRESET_PALETTES.map((name) => (
                    <button
                        key={name}
                        style={{ ...BTN_GHOST, fontSize: 10 }}
                        onClick={() => sendPalettePrompt(name)}
                        disabled={isLoading}
                    >
                        {name}
                    </button>
                ))}
            </div>

            {/* Last applied swatch */}
            {lastPalette && (
                <div style={{ display: "flex", gap: 4, alignItems: "center" }}>
                    <span style={{ fontSize: 10, color: "#6c7086" }}>Last: {lastPalette.label}</span>
                    {(["background", "primary", "accent"] as const).map((k) => (
                        <div
                            key={k}
                            title={`${k}: ${lastPalette[k]}`}
                            style={{
                                width: 14, height: 14, borderRadius: 3,
                                background: lastPalette[k],
                                border: "1px solid #45475a",
                            }}
                        />
                    ))}
                </div>
            )}

            <div style={{ display: "flex", justifyContent: "flex-end" }}>
                <button style={{ ...BTN_PRIMARY, opacity: isLoading ? 0.5 : 1 }} onClick={handleSubmit} disabled={isLoading}>
                    {isLoading ? "Applying…" : "Apply"}
                </button>
            </div>
        </div>
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Breakpoints tab
// ─────────────────────────────────────────────────────────────────────────────

const PRESET_BREAKPOINTS = [
    { label: "Mobile + Tablet + Desktop", widths: [375, 768, 1440] },
    { label: "All 4", widths: [375, 768, 1024, 1440] },
    { label: "Mobile only", widths: [375, 414] },
];

function BreakpointsTab() {
    const [frameId, setFrameId] = useState("");
    const [widthsStr, setWidthsStr] = useState("375, 768, 1440");
    const sendBreakpointPrompt = useAiStore((s) => s.sendBreakpointPrompt);
    const isLoading = useAiStore((s) => s.isLoading);

    const handleSubmit = useCallback(() => {
        if (!frameId.trim() || isLoading) return;
        const widths = widthsStr
            .split(/[,\s]+/)
            .map(Number)
            .filter((n) => n > 0);
        if (widths.length === 0) return;
        sendBreakpointPrompt(frameId.trim(), widths);
    }, [frameId, widthsStr, isLoading, sendBreakpointPrompt]);

    return (
        <div style={CONTROLS}>
            <div>
                <div style={LABEL}>Frame ID or name</div>
                <input
                    style={INPUT}
                    value={frameId}
                    onChange={(e) => setFrameId(e.target.value)}
                    placeholder="e.g. Landing Page or uuid"
                    disabled={isLoading}
                />
            </div>
            <div>
                <div style={LABEL}>Widths (px, comma-separated)</div>
                <input
                    style={INPUT}
                    value={widthsStr}
                    onChange={(e) => setWidthsStr(e.target.value)}
                    placeholder="375, 768, 1024, 1440"
                    disabled={isLoading}
                />
            </div>

            {/* Presets */}
            <div style={{ display: "flex", flexWrap: "wrap", gap: 4 }}>
                {PRESET_BREAKPOINTS.map((p) => (
                    <button
                        key={p.label}
                        style={{ ...BTN_GHOST, fontSize: 10 }}
                        onClick={() => setWidthsStr(p.widths.join(", "))}
                        disabled={isLoading}
                    >
                        {p.label}
                    </button>
                ))}
            </div>

            <div style={{ display: "flex", justifyContent: "flex-end" }}>
                <button style={{ ...BTN_PRIMARY, opacity: isLoading ? 0.5 : 1 }} onClick={handleSubmit} disabled={isLoading}>
                    {isLoading ? "Creating…" : "Create Breakpoints"}
                </button>
            </div>
        </div>
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Main panel
// ─────────────────────────────────────────────────────────────────────────────

const TABS_CONFIG: { id: AiTab; label: string }[] = [
    { id: "generate", label: "Generate" },
    { id: "palette", label: "Palette" },
    { id: "breakpoints", label: "Breakpoints" },
];

export function AIPanel() {
    const history = useAiStore((s) => s.history);
    const activeTab = useAiStore((s) => s.activeTab);
    const setActiveTab = useAiStore((s) => s.setActiveTab);
    const clearHistory = useAiStore((s) => s.clearHistory);
    const historyRef = useRef<HTMLDivElement>(null);

    // Scroll to bottom when new messages arrive
    useEffect(() => {
        if (historyRef.current) {
            historyRef.current.scrollTop = historyRef.current.scrollHeight;
        }
    }, [history.length]);

    return (
        <div style={PANEL}>
            {/* Header + tabs */}
            <div style={HEADER}>
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                    <div style={TITLE}>AI Assistant</div>
                    {history.length > 0 && (
                        <button style={BTN_GHOST} onClick={clearHistory} title="Clear history">
                            Clear
                        </button>
                    )}
                </div>
                <div style={TABS}>
                    {TABS_CONFIG.map(({ id, label }) => (
                        <button
                            key={id}
                            style={activeTab === id ? TAB_ACTIVE : TAB_BASE}
                            onClick={() => setActiveTab(id)}
                        >
                            {label}
                        </button>
                    ))}
                </div>
            </div>

            {/* Tab controls */}
            <div style={BODY}>
                {activeTab === "generate" && <GenerateTab />}
                {activeTab === "palette" && <PaletteTab />}
                {activeTab === "breakpoints" && <BreakpointsTab />}

                {/* Message history */}
                <div ref={historyRef} style={HISTORY}>
                    {history.length === 0 ? (
                        <div style={{ color: "#45475a", fontSize: 12, textAlign: "center", marginTop: 24 }}>
                            No history yet. Try generating a layout above.
                        </div>
                    ) : (
                        history.map((msg) => <MessageBubble key={msg.id} msg={msg} />)
                    )}
                </div>
            </div>
        </div>
    );
}
