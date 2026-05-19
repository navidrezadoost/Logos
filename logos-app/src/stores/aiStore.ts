/**
 * stores/aiStore.ts
 *
 * AI Design Assistant state: prompt history, loading, in-app interpreter.
 * The in-app interpreter maps natural language to documentStore operations
 * so basic generation works without an external LLM or MCP connection.
 */

import { create } from "zustand";
import { type Shape, IDENTITY_TRANSFORM } from "../types/shapes";
import { useDocumentStore } from "./documentStore";

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

export type AiMessageRole = "user" | "assistant" | "error";

export interface AiMessage {
    id: string;
    role: AiMessageRole;
    content: string;
    timestamp: number;
    createdShapeIds?: string[];
}

export type AiTab = "generate" | "palette" | "breakpoints";

export interface ResolvedPalette {
    label: string;
    background: string;
    surface: string;
    primary: string;
    secondary: string;
    text: string;
    muted: string;
    accent: string;
    border: string;
}

interface AiState {
    history: AiMessage[];
    isLoading: boolean;
    error: string | null;
    activeTab: AiTab;
    lastPalette: ResolvedPalette | null;

    setActiveTab: (tab: AiTab) => void;
    clearHistory: () => void;
    setError: (e: string | null) => void;
    sendGeneratePrompt: (prompt: string, x?: number, y?: number, width?: number) => void;
    sendPalettePrompt: (description: string, shapeIds?: string[]) => void;
    sendBreakpointPrompt: (frameId: string, widths: number[]) => void;
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

function solidFill(color: string): Shape["fills"] {
    return [{ type: "solid", color, opacity: 1 }];
}

function makeRect(
    name: string,
    x: number, y: number,
    w: number, h: number,
    color: string
): Shape {
    return {
        id: crypto.randomUUID(),
        type: "rect",
        name,
        bounds: { x, y, w, h },
        transform: IDENTITY_TRANSFORM,
        rotation: 0,
        fills: solidFill(color),
        opacity: 1,
        hidden: false,
        locked: false,
        parentId: null,
        children: [],
    };
}

function pushMsg(
    set: (fn: (s: AiState) => Partial<AiState>) => void,
    role: AiMessageRole,
    content: string,
    extras?: Partial<AiMessage>
): void {
    const msg: AiMessage = {
        id: crypto.randomUUID(),
        role,
        content,
        timestamp: Date.now(),
        ...extras,
    };
    set((s) => ({ history: [...s.history, msg] }));
}

// ─────────────────────────────────────────────────────────────────────────────
// In-app layout templates
// ─────────────────────────────────────────────────────────────────────────────

interface Template {
    keywords: string[];
    label: string;
    generate: (add: (s: Shape) => void, x: number, y: number, width: number) => void;
}

const TEMPLATES: Template[] = [
    {
        keywords: ["hero", "landing", "banner", "jumbotron"],
        label: "Hero section",
        generate(add, x, y, w) {
            const pad = 60;
            add(makeRect("Hero / Background", x, y, w, 480, "#1e1e2e"));
            add(makeRect("Hero / Heading", x + pad, y + 120, w - pad * 2, 64, "#4a5568"));
            add(makeRect("Hero / Subheading", x + pad, y + 200, w - pad * 2, 36, "#313244"));
            add(makeRect("Hero / Primary CTA", x + pad, y + 280, 160, 48, "#89b4fa"));
            add(makeRect("Hero / Secondary CTA", x + pad + 176, y + 280, 160, 48, "#313244"));
        },
    },
    {
        keywords: ["login", "sign in", "signin", "auth", "authentication"],
        label: "Login form",
        generate(add, x, y, w) {
            const fw = Math.min(w, 400);
            const cx = x + (w - fw) / 2;
            add(makeRect("Login / Frame", cx, y, fw, 420, "#1e1e2e"));
            add(makeRect("Login / Email Input", cx + 32, y + 130, fw - 64, 44, "#313244"));
            add(makeRect("Login / Password Input", cx + 32, y + 222, fw - 64, 44, "#313244"));
            add(makeRect("Login / Submit", cx + 32, y + 300, fw - 64, 48, "#89b4fa"));
        },
    },
    {
        keywords: ["card", "cards", "grid", "column", "columns", "three"],
        label: "Card grid (3-col)",
        generate(add, x, y, w) {
            const cols = 3, gap = 24;
            const cardW = Math.floor((w - gap * (cols + 1)) / cols);
            for (let i = 0; i < cols; i++) {
                const cx = x + gap + i * (cardW + gap);
                add(makeRect(`Card ${i + 1} / Frame`, cx, y + gap, cardW, 280, "#1e1e2e"));
                add(makeRect(`Card ${i + 1} / Image`, cx, y + gap, cardW, 140, "#313244"));
                add(makeRect(`Card ${i + 1} / CTA`, cx + 16, y + gap + 232, 96, 32, "#89b4fa"));
            }
        },
    },
    {
        keywords: ["nav", "navbar", "navigation", "header", "menu"],
        label: "Navigation bar",
        generate(add, x, y, w) {
            add(makeRect("Navbar / Background", x, y, w, 64, "#181825"));
            add(makeRect("Navbar / Logo", x + 24, y + 18, 100, 28, "#89b4fa"));
            add(makeRect("Navbar / Links", x + w - 340, y + 18, 316, 28, "#1e1e2e"));
        },
    },
    {
        keywords: ["form", "input", "contact", "contact form"],
        label: "Contact form",
        generate(add, x, y, w) {
            const fw = Math.min(w, 560);
            const cx = x + (w - fw) / 2;
            add(makeRect("Contact / Frame", cx, y, fw, 480, "#1e1e2e"));
            add(makeRect("Contact / Name Input", cx + 32, y + 96, fw - 64, 44, "#313244"));
            add(makeRect("Contact / Email Input", cx + 32, y + 172, fw - 64, 44, "#313244"));
            add(makeRect("Contact / Message Input", cx + 32, y + 248, fw - 64, 100, "#313244"));
            add(makeRect("Contact / Send", cx + 32, y + 380, fw - 64, 48, "#89b4fa"));
        },
    },
];

function runTemplate(
    prompt: string,
    x: number,
    y: number,
    width: number
): { label: string; shapeIds: string[] } | null {
    const lower = prompt.toLowerCase();
    const tpl = TEMPLATES.find((t) => t.keywords.some((kw) => lower.includes(kw)));
    if (!tpl) return null;

    const { addShape } = useDocumentStore.getState();
    const shapeIds: string[] = [];
    tpl.generate(
        (s: Shape) => { addShape(s); shapeIds.push(s.id); },
        x, y, width
    );
    return { label: tpl.label, shapeIds };
}

// ─────────────────────────────────────────────────────────────────────────────
// Palette definitions
// ─────────────────────────────────────────────────────────────────────────────

interface PaletteEntry extends ResolvedPalette {
    keywords: string[];
}

const PALETTES: PaletteEntry[] = [
    {
        label: "Dark Mode", keywords: ["dark", "night", "catppuccin", "mocha"],
        background: "#1e1e2e", surface: "#313244", primary: "#89b4fa",
        secondary: "#cba6f7", text: "#cdd6f4", muted: "#a6adc8",
        accent: "#f38ba8", border: "#45475a",
    },
    {
        label: "Warm Earth", keywords: ["warm", "earth", "tan", "terracotta"],
        background: "#faf7f2", surface: "#f0e6d3", primary: "#c17f52",
        secondary: "#a0522d", text: "#3d2b1f", muted: "#7a5c46",
        accent: "#e07b39", border: "#d4b896",
    },
    {
        label: "Ocean Blue", keywords: ["ocean", "sea", "teal", "marine", "blue"],
        background: "#081c24", surface: "#0d2f3f", primary: "#28c7d9",
        secondary: "#1a8fa0", text: "#e0f4f8", muted: "#89c4cc",
        accent: "#ff6b6b", border: "#1f4f60",
    },
    {
        label: "Pastel", keywords: ["pastel", "soft", "gentle", "cotton"],
        background: "#fefefe", surface: "#f8f4ff", primary: "#b5a9f2",
        secondary: "#f2a9c5", text: "#3a3251", muted: "#9b92b8",
        accent: "#a9d4f2", border: "#e0d8ff",
    },
    {
        label: "Sunset", keywords: ["sunset", "dusk", "orange", "purple gradient"],
        background: "#1a0a2e", surface: "#2d1454", primary: "#e8517a",
        secondary: "#f4a261", text: "#f8e8ff", muted: "#b88acf",
        accent: "#ff9f43", border: "#4a2064",
    },
    {
        label: "Light / Clean", keywords: ["light", "white", "clean", "minimal", "day"],
        background: "#ffffff", surface: "#f5f5f5", primary: "#3b82f6",
        secondary: "#6366f1", text: "#111827", muted: "#6b7280",
        accent: "#ef4444", border: "#e5e7eb",
    },
];

function resolvePalette(description: string): PaletteEntry | null {
    const lower = description.toLowerCase();
    return PALETTES.find((p) => p.keywords.some((kw) => lower.includes(kw))) ?? null;
}

// ─────────────────────────────────────────────────────────────────────────────
// Store
// ─────────────────────────────────────────────────────────────────────────────

export const useAiStore = create<AiState>((set) => ({
    history: [],
    isLoading: false,
    error: null,
    activeTab: "generate",
    lastPalette: null,

    setActiveTab: (activeTab) => set({ activeTab }),
    clearHistory: () => set({ history: [], error: null }),
    setError: (error) => set({ error }),

    sendGeneratePrompt(prompt, x = 0, y = 0, width = 800) {
        if (!prompt.trim()) return;
        set({ isLoading: true, error: null });
        pushMsg(set, "user", prompt);

        setTimeout(() => {
            try {
                const result = runTemplate(prompt, x, y, width);
                if (!result) {
                    pushMsg(set, "assistant",
                        `I couldn't recognise a layout type in "${prompt}". ` +
                        `Try: hero section, login form, card grid, navigation bar, or contact form.`);
                } else {
                    pushMsg(set, "assistant",
                        `Generated **${result.label}** with ${result.shapeIds.length} shapes at (${x}, ${y}).`,
                        { createdShapeIds: result.shapeIds });
                }
            } catch (e) {
                const msg = e instanceof Error ? e.message : String(e);
                pushMsg(set, "error", `Error: ${msg}`);
                set({ error: msg });
            } finally {
                set({ isLoading: false });
            }
        }, 0);
    },

    sendPalettePrompt(description, shapeIds) {
        if (!description.trim()) return;
        set({ isLoading: true, error: null });
        pushMsg(set, "user", `Apply palette: ${description}`);

        setTimeout(() => {
            try {
                const palette = resolvePalette(description);
                if (!palette) {
                    pushMsg(set, "assistant",
                        `No palette matched "${description}". Available: ${PALETTES.map((p) => p.label).join(", ")}.`);
                } else {
                    const { shapes, updateShape } = useDocumentStore.getState();
                    const allShapes = Object.values(shapes);
                    const targets: Shape[] =
                        shapeIds && shapeIds.length > 0
                            ? shapeIds.map((id) => shapes[id]).filter((s): s is Shape => Boolean(s))
                            : allShapes;

                    const fillKeys: (keyof ResolvedPalette)[] = [
                        "background", "surface", "primary", "secondary", "accent", "muted", "text",
                    ];

                    targets.forEach((shape, i) => {
                        const colorKey = fillKeys[i % fillKeys.length];
                        updateShape(shape.id, { fills: solidFill(palette[colorKey]) });
                    });

                    set({ lastPalette: palette });
                    pushMsg(set, "assistant",
                        `Applied **${palette.label}** to ${targets.length} shape(s). Primary: ${palette.primary}.`);
                }
            } catch (e) {
                const msg = e instanceof Error ? e.message : String(e);
                pushMsg(set, "error", `Error: ${msg}`);
                set({ error: msg });
            } finally {
                set({ isLoading: false });
            }
        }, 0);
    },

    sendBreakpointPrompt(frameId, widths) {
        if (!frameId.trim() || widths.length === 0) return;
        set({ isLoading: true, error: null });
        pushMsg(set, "user", `Create breakpoints for "${frameId}" at [${widths.join(", ")}]px`);

        setTimeout(() => {
            try {
                const { shapes, addShape } = useDocumentStore.getState();
                const allShapes = Object.values(shapes);
                const source = shapes[frameId] ?? allShapes.find((s) => s.name === frameId);

                if (!source) {
                    pushMsg(set, "assistant",
                        `Frame "${frameId}" not found. Use the exact ID or name from the Layers panel.`);
                    set({ isLoading: false });
                    return;
                }

                const { x: sx, y: sy, w: sw, h: sh } = source.bounds;
                const gap = 80;
                const rowY = sy + sh + gap;
                let cursorX = sx;
                const created: string[] = [];

                for (const targetW of widths) {
                    const scale = targetW / sw;
                    const dup: Shape = {
                        ...source,
                        id: crypto.randomUUID(),
                        name: `${source.name} — ${targetW}px`,
                        bounds: { x: cursorX, y: rowY, w: targetW, h: Math.round(sh * scale) },
                        children: [],
                    };
                    addShape(dup);
                    created.push(dup.name);
                    cursorX += targetW + gap;
                }

                pushMsg(set, "assistant",
                    `Created ${created.length} breakpoint frame(s): ${created.join(", ")}.`);
            } catch (e) {
                const msg = e instanceof Error ? e.message : String(e);
                pushMsg(set, "error", `Error: ${msg}`);
                set({ error: msg });
            } finally {
                set({ isLoading: false });
            }
        }, 0);
    },
}));
