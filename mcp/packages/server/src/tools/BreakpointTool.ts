import { z } from "zod";
import { Tool } from "../Tool";
import type { ToolResponse } from "../ToolResponse";
import { TextResponse } from "../ToolResponse";
import { LogosMcpServer } from "../LogosMcpServer";
import { ExecuteCodePluginTask } from "../tasks/ExecuteCodePluginTask";

// ─────────────────────────────────────────────────────────────────────────────
// Args
// ─────────────────────────────────────────────────────────────────────────────

export class BreakpointArgs {
    static schema = {
        frameId: z
            .string()
            .min(1, "frameId cannot be empty")
            .describe(
                "The ID of the source frame to duplicate at each breakpoint width. " +
                "Use the shape ID from the Layers panel."
            ),
        widths: z
            .array(z.number().positive())
            .min(1, "Provide at least one breakpoint width")
            .max(8, "Maximum 8 breakpoints per call")
            .describe(
                "Array of target widths in pixels. " +
                "Common values: [375, 768, 1024, 1440] for mobile, tablet, laptop, desktop."
            ),
        gap: z
            .number()
            .optional()
            .describe(
                "Horizontal gap between duplicated frames in pixels (default: 80)."
            ),
        scaleContents: z
            .boolean()
            .optional()
            .describe(
                "When true, child shapes are scaled proportionally to the new width (default: true)."
            ),
    };

    frameId!: string;
    widths!: number[];
    gap?: number;
    scaleContents?: boolean;
}

// ─────────────────────────────────────────────────────────────────────────────
// Code generator
// ─────────────────────────────────────────────────────────────────────────────

function buildBreakpointCode(
    frameId: string,
    widths: number[],
    gap: number,
    scaleContents: boolean
): string {
    return `
(async () => {
  const sourceId = ${JSON.stringify(frameId)};
  const targetWidths = ${JSON.stringify(widths)};
  const gap = ${gap};
  const scaleContents = ${scaleContents};

  // Resolve source frame
  const source = logos.currentPage.getShapeById(sourceId);
  if (!source) {
    return { error: "Frame not found: " + sourceId };
  }

  const sourceWidth = source.width;
  const sourceHeight = source.height;
  const sourceX = source.x;
  const sourceY = source.y;

  // Place duplicates to the right of the source, then below
  const created = [];

  // Calculate row offset: put breakpoints below the source frame
  const rowY = sourceY + sourceHeight + gap;
  let cursorX = sourceX;

  for (let i = 0; i < targetWidths.length; i++) {
    const targetWidth = targetWidths[i];
    const scale = targetWidth / sourceWidth;
    const targetHeight = Math.round(sourceHeight * scale);

    // Duplicate
    const dup = source.clone();
    dup.name = source.name + " — " + targetWidth + "px";
    dup.x = cursorX;
    dup.y = rowY;
    dup.width = targetWidth;
    dup.height = targetHeight;

    // Scale children if requested
    if (scaleContents && dup.getChildren) {
      const children = dup.getChildren();
      for (const child of children) {
        const relX = (child.x - source.x) / sourceWidth;
        const relY = (child.y - source.y) / sourceHeight;
        const relW = child.width / sourceWidth;
        const relH = child.height / sourceHeight;

        child.x = dup.x + relX * targetWidth;
        child.y = dup.y + relY * targetHeight;
        child.width = relW * targetWidth;
        child.height = relH * targetHeight;

        // Scale font size proportionally
        if (child.fontSize) {
          child.fontSize = Math.max(8, Math.round(child.fontSize * scale));
        }
      }
    }

    created.push({
      name: dup.name,
      id: dup.id,
      width: targetWidth,
      height: targetHeight,
      x: cursorX,
      y: rowY,
    });

    cursorX += targetWidth + gap;
  }

  return {
    source: { id: sourceId, name: source.name, width: sourceWidth, height: sourceHeight },
    breakpoints: created,
  };
})()
`;
}

// ─────────────────────────────────────────────────────────────────────────────
// Tool
// ─────────────────────────────────────────────────────────────────────────────

export class BreakpointTool extends Tool<BreakpointArgs> {
    constructor(mcpServer: LogosMcpServer) {
        super(mcpServer, BreakpointArgs.schema);
    }

    public getToolName(): string {
        return "create_breakpoints";
    }

    public getToolDescription(): string {
        return (
            "Duplicates a Logos frame at multiple target widths to create responsive design breakpoints.\n" +
            "Each duplicate is placed below the source frame, side by side, with an optional gap.\n" +
            "Child shapes can be proportionally scaled to the new width.\n" +
            "Parameters:\n" +
            "  frameId       — ID of the source frame (required)\n" +
            "  widths        — array of target widths, e.g. [375, 768, 1024, 1440]\n" +
            "  gap           — horizontal gap between frames in pixels (default: 80)\n" +
            "  scaleContents — scale child shapes proportionally (default: true)\n" +
            "Returns positions and IDs of all created frames."
        );
    }

    protected async executeCore(args: BreakpointArgs): Promise<ToolResponse> {
        const gap = args.gap ?? 80;
        const scaleContents = args.scaleContents ?? true;

        const code = buildBreakpointCode(
            args.frameId,
            args.widths,
            gap,
            scaleContents
        );

        const task = new ExecuteCodePluginTask({ code });
        const result = await this.mcpServer.pluginBridge.executePluginTask(task);

        return new TextResponse(
            `Created ${args.widths.length} breakpoint frame(s) at widths: ${args.widths.join(", ")}px\n\n` +
            `Result: ${JSON.stringify(result.data, null, 2)}`
        );
    }
}
