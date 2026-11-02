import { z } from "zod";
import { Tool } from "../Tool";
import type { ToolResponse } from "../ToolResponse";
import { TextResponse } from "../ToolResponse";
import { LogosMcpServer } from "../LogosMcpServer";
import { ExecuteCodePluginTask } from "../tasks/ExecuteCodePluginTask";

// ─────────────────────────────────────────────────────────────────────────────
// Args
// ─────────────────────────────────────────────────────────────────────────────

export class ApplyPaletteArgs {
    static schema = {
        description: z
            .string()
            .min(1, "Description cannot be empty")
            .describe(
                "Semantic color palette description. " +
                "Examples: 'dark mode', 'warm earth tones', 'ocean blue', " +
                "'pastel', 'high contrast', 'sunset gradient', 'monochrome'."
            ),
        shapeIds: z
            .array(z.string())
            .optional()
            .describe(
                "Array of shape IDs to apply the palette to. " +
                "If omitted, the palette is applied to the current selection."
            ),
        mode: z
            .enum(["fills", "strokes", "both"])
            .optional()
            .describe(
                "Which property to update: 'fills', 'strokes', or 'both' (default: 'fills')."
            ),
    };

    description!: string;
    shapeIds?: string[];
    mode?: "fills" | "strokes" | "both";
}

// ─────────────────────────────────────────────────────────────────────────────
// Palette definitions
// ─────────────────────────────────────────────────────────────────────────────

interface ColorSet {
    background: string;
    surface: string;
    primary: string;
    secondary: string;
    text: string;
    muted: string;
    accent: string;
    border: string;
}

type PaletteEntry = { keywords: string[]; label: string; colors: ColorSet };

const PALETTES: PaletteEntry[] = [
    {
        keywords: ["dark", "night", "catppuccin", "mocha"],
        label: "Dark Mode (Catppuccin Mocha)",
        colors: {
            background: "#1e1e2e",
            surface: "#313244",
            primary: "#89b4fa",
            secondary: "#cba6f7",
            text: "#cdd6f4",
            muted: "#a6adc8",
            accent: "#f38ba8",
            border: "#45475a",
        },
    },
    {
        keywords: ["warm", "earth", "tan", "terracotta", "rustic"],
        label: "Warm Earth Tones",
        colors: {
            background: "#faf7f2",
            surface: "#f0e6d3",
            primary: "#c17f52",
            secondary: "#a0522d",
            text: "#3d2b1f",
            muted: "#7a5c46",
            accent: "#e07b39",
            border: "#d4b896",
        },
    },
    {
        keywords: ["ocean", "sea", "teal", "marine", "blue"],
        label: "Ocean Blue",
        colors: {
            background: "#081c24",
            surface: "#0d2f3f",
            primary: "#28c7d9",
            secondary: "#1a8fa0",
            text: "#e0f4f8",
            muted: "#89c4cc",
            accent: "#ff6b6b",
            border: "#1f4f60",
        },
    },
    {
        keywords: ["pastel", "soft", "gentle", "light", "cotton"],
        label: "Pastel",
        colors: {
            background: "#fefefe",
            surface: "#f8f4ff",
            primary: "#b5a9f2",
            secondary: "#f2a9c5",
            text: "#3a3251",
            muted: "#9b92b8",
            accent: "#a9d4f2",
            border: "#e0d8ff",
        },
    },
    {
        keywords: ["high contrast", "accessibility", "black and white", "monochrome"],
        label: "High Contrast",
        colors: {
            background: "#000000",
            surface: "#111111",
            primary: "#ffffff",
            secondary: "#dddddd",
            text: "#ffffff",
            muted: "#aaaaaa",
            accent: "#ffff00",
            border: "#444444",
        },
    },
    {
        keywords: ["sunset", "gradient", "orange", "purple", "dusk"],
        label: "Sunset",
        colors: {
            background: "#1a0a2e",
            surface: "#2d1454",
            primary: "#e8517a",
            secondary: "#f4a261",
            text: "#f8e8ff",
            muted: "#b88acf",
            accent: "#ff9f43",
            border: "#4a2064",
        },
    },
    {
        keywords: ["green", "forest", "nature", "plant", "eco"],
        label: "Forest Green",
        colors: {
            background: "#0f1f14",
            surface: "#1c3e25",
            primary: "#4caf50",
            secondary: "#81c784",
            text: "#e8f5e9",
            muted: "#a5d6a7",
            accent: "#ffcc02",
            border: "#2e6b38",
        },
    },
    {
        keywords: ["light", "white", "clean", "minimal", "day"],
        label: "Light / Clean",
        colors: {
            background: "#ffffff",
            surface: "#f5f5f5",
            primary: "#3b82f6",
            secondary: "#6366f1",
            text: "#111827",
            muted: "#6b7280",
            accent: "#ef4444",
            border: "#e5e7eb",
        },
    },
];

// ─────────────────────────────────────────────────────────────────────────────
// Code generator
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Generate plugin JS code that distributes palette colors across shapes.
 * The first shape gets background, subsequent ones cycle through surface/primary/secondary.
 */
function buildPaletteCode(
    palette: ColorSet,
    shapeIds: string[] | undefined,
    mode: "fills" | "strokes" | "both"
): string {
    const ids = shapeIds ? JSON.stringify(shapeIds) : "null";
    const palJson = JSON.stringify(palette, null, 2);
    const modeStr = mode;

    return `
(async () => {
  const palette = ${palJson};
  const requestedIds = ${ids};
  const mode = "${modeStr}";

  // Resolve targets
  let shapes;
  if (requestedIds) {
    shapes = requestedIds
      .map((id) => logos.currentPage.getShapeById(id))
      .filter(Boolean);
  } else {
    shapes = logos.selection;
  }

  if (!shapes || shapes.length === 0) {
    return { error: "No shapes found. Select shapes or pass shapeIds." };
  }

  // Distribute colors
  const fillKeys = ["background", "surface", "primary", "secondary", "accent", "muted"];
  const updated = [];

  shapes.forEach((shape, i) => {
    const colorKey = fillKeys[i % fillKeys.length];
    const color = palette[colorKey];
    const fill = [{ fillType: "solid", fillColor: color, fillOpacity: 1 }];
    const stroke = [{ strokeColor: palette.border, strokeWidth: 1, strokeType: "inner" }];

    if (mode === "fills" || mode === "both") {
      shape.fills = fill;
    }
    if (mode === "strokes" || mode === "both") {
      shape.strokes = stroke;
    }
    updated.push({ id: shape.id, name: shape.name, color });
  });

  return { palette: Object.keys(palette).reduce((acc, k) => { acc[k] = palette[k]; return acc; }, {}), updated };
})()
`;
}

// ─────────────────────────────────────────────────────────────────────────────
// Tool
// ─────────────────────────────────────────────────────────────────────────────

export class ApplyPaletteTool extends Tool<ApplyPaletteArgs> {
    constructor(mcpServer: LogosMcpServer) {
        super(mcpServer, ApplyPaletteArgs.schema);
    }

    public getToolName(): string {
        return "apply_palette";
    }

    public getToolDescription(): string {
        return (
            "Applies a semantic color palette to shapes on the Logos canvas.\n" +
            "Supported palettes: dark mode, warm earth tones, ocean blue, pastel, " +
            "high contrast, sunset, forest green, light/clean.\n" +
            "Parameters:\n" +
            "  description — semantic color description (required)\n" +
            "  shapeIds    — shape IDs to recolor (default: current selection)\n" +
            "  mode        — 'fills', 'strokes', or 'both' (default: 'fills')\n" +
            "Returns the palette hex values and a list of updated shapes."
        );
    }

    protected async executeCore(args: ApplyPaletteArgs): Promise<ToolResponse> {
        const desc = args.description.toLowerCase();
        const mode = args.mode ?? "fills";

        const matched = PALETTES.find((p) =>
            p.keywords.some((kw) => desc.includes(kw))
        );

        if (!matched) {
            const available = PALETTES.map((p) => `"${p.label}"`).join(", ");
            return new TextResponse(
                `No palette matched "${args.description}".\n` +
                `Available palettes: ${available}.`
            );
        }

        const code = buildPaletteCode(matched.colors, args.shapeIds, mode);
        const task = new ExecuteCodePluginTask({ code });
        const result = await this.mcpServer.pluginBridge.executePluginTask(task);

        return new TextResponse(
            `Applied palette: "${matched.label}"\n\n` +
            `Result: ${JSON.stringify(result.data, null, 2)}`
        );
    }
}
