import { z } from "zod";
import { Tool } from "../Tool";
import type { ToolResponse } from "../ToolResponse";
import { TextResponse } from "../ToolResponse";
import { LogosMcpServer } from "../LogosMcpServer";
import { ExecuteCodePluginTask } from "../tasks/ExecuteCodePluginTask";

// ─────────────────────────────────────────────────────────────────────────────
// Args
// ─────────────────────────────────────────────────────────────────────────────

export class GenerateLayoutArgs {
    static schema = {
        prompt: z
            .string()
            .min(1, "Prompt cannot be empty")
            .describe(
                "Natural language description of the layout to generate. " +
                "Examples: 'a hero section with heading, subheading, and two buttons', " +
                "'a login form with email, password, and submit button', " +
                "'a three-column card grid with title, body, and CTA'."
            ),
        x: z
            .number()
            .optional()
            .describe("X position for the generated frame (default: 0)."),
        y: z
            .number()
            .optional()
            .describe("Y position for the generated frame (default: 0)."),
        width: z
            .number()
            .optional()
            .describe("Width of the generated frame in pixels (default: 800)."),
    };

    prompt!: string;
    x?: number;
    y?: number;
    width?: number;
}

// ─────────────────────────────────────────────────────────────────────────────
// Layout templates
//
// Each template is a function (x, y, width) → JavaScript plugin code that
// creates shapes via the Logos plugin API. The code is executed in the plugin
// sandbox via ExecuteCodePluginTask — the same mechanism used by ExecuteCodeTool.
// ─────────────────────────────────────────────────────────────────────────────

type Template = (x: number, y: number, width: number) => string;

const TEMPLATES: Array<{ keywords: string[]; label: string; code: Template }> = [
    {
        keywords: ["hero", "landing", "banner", "jumbotron"],
        label: "Hero section",
        code: (x, y, w) => `
(async () => {
  const pad = 60;
  const frame = logos.createFrame();
  frame.name = "Hero Section";
  frame.x = ${x}; frame.y = ${y};
  frame.width = ${w}; frame.height = 480;
  frame.fills = [{ fillType: 'solid', fillColor: '#1e1e2e', fillOpacity: 1 }];

  const heading = logos.createText("The Future of Design");
  heading.name = "Heading";
  heading.x = ${x} + pad; heading.y = ${y} + 120;
  heading.width = ${w} - pad * 2; heading.height = 64;
  heading.fontSize = 48; heading.fontWeight = "700";
  heading.fills = [{ fillType: 'solid', fillColor: '#cdd6f4', fillOpacity: 1 }];

  const sub = logos.createText("Open-source design, powered by Rust and AI.");
  sub.name = "Subheading";
  sub.x = ${x} + pad; sub.y = ${y} + 200;
  sub.width = ${w} - pad * 2; sub.height = 36;
  sub.fontSize = 20; sub.fontWeight = "400";
  sub.fills = [{ fillType: 'solid', fillColor: '#a6adc8', fillOpacity: 1 }];

  const btn1 = logos.createRect();
  btn1.name = "Primary CTA";
  btn1.x = ${x} + pad; btn1.y = ${y} + 280;
  btn1.width = 160; btn1.height = 48;
  btn1.fills = [{ fillType: 'solid', fillColor: '#89b4fa', fillOpacity: 1 }];
  btn1.borderRadius = 8;

  const btn2 = logos.createRect();
  btn2.name = "Secondary CTA";
  btn2.x = ${x} + pad + 176; btn2.y = ${y} + 280;
  btn2.width = 160; btn2.height = 48;
  btn2.fills = [{ fillType: 'solid', fillColor: 'transparent', fillOpacity: 0 }];
  btn2.borderRadius = 8; btn2.strokes = [{ strokeColor: '#6c7086', strokeWidth: 1 }];

  return { created: ["Hero Section", "Heading", "Subheading", "Primary CTA", "Secondary CTA"] };
})()
`,
    },
    {
        keywords: ["login", "sign in", "signin", "auth", "authentication"],
        label: "Login form",
        code: (x, y, w) => `
(async () => {
  const fw = Math.min(${w}, 400);
  const cx = ${x} + (${w} - fw) / 2;

  const frame = logos.createFrame();
  frame.name = "Login Form";
  frame.x = cx; frame.y = ${y};
  frame.width = fw; frame.height = 420;
  frame.fills = [{ fillType: 'solid', fillColor: '#1e1e2e', fillOpacity: 1 }];
  frame.borderRadius = 12;

  const title = logos.createText("Sign in");
  title.name = "Title"; title.x = cx + 32; title.y = ${y} + 40;
  title.width = fw - 64; title.height = 40;
  title.fontSize = 28; title.fontWeight = "700";
  title.fills = [{ fillType: 'solid', fillColor: '#cdd6f4', fillOpacity: 1 }];

  // Email field
  const emailLabel = logos.createText("Email");
  emailLabel.name = "Email Label"; emailLabel.x = cx + 32; emailLabel.y = ${y} + 108;
  emailLabel.width = fw - 64; emailLabel.height = 20; emailLabel.fontSize = 13;
  emailLabel.fills = [{ fillType: 'solid', fillColor: '#a6adc8', fillOpacity: 1 }];

  const emailInput = logos.createRect();
  emailInput.name = "Email Input"; emailInput.x = cx + 32; emailInput.y = ${y} + 132;
  emailInput.width = fw - 64; emailInput.height = 44;
  emailInput.fills = [{ fillType: 'solid', fillColor: '#313244', fillOpacity: 1 }];
  emailInput.borderRadius = 6;

  // Password field
  const pwLabel = logos.createText("Password");
  pwLabel.name = "Password Label"; pwLabel.x = cx + 32; pwLabel.y = ${y} + 200;
  pwLabel.width = fw - 64; pwLabel.height = 20; pwLabel.fontSize = 13;
  pwLabel.fills = [{ fillType: 'solid', fillColor: '#a6adc8', fillOpacity: 1 }];

  const pwInput = logos.createRect();
  pwInput.name = "Password Input"; pwInput.x = cx + 32; pwInput.y = ${y} + 224;
  pwInput.width = fw - 64; pwInput.height = 44;
  pwInput.fills = [{ fillType: 'solid', fillColor: '#313244', fillOpacity: 1 }];
  pwInput.borderRadius = 6;

  const submitBtn = logos.createRect();
  submitBtn.name = "Submit Button"; submitBtn.x = cx + 32; submitBtn.y = ${y} + 300;
  submitBtn.width = fw - 64; submitBtn.height = 48;
  submitBtn.fills = [{ fillType: 'solid', fillColor: '#89b4fa', fillOpacity: 1 }];
  submitBtn.borderRadius = 8;

  const submitLabel = logos.createText("Sign in");
  submitLabel.name = "Submit Label"; submitLabel.x = cx + 32; submitLabel.y = ${y} + 312;
  submitLabel.width = fw - 64; submitLabel.height = 24; submitLabel.fontSize = 15;
  submitLabel.fontWeight = "600"; submitLabel.textAlign = "center";
  submitLabel.fills = [{ fillType: 'solid', fillColor: '#1e1e2e', fillOpacity: 1 }];

  return { created: ["Login Form", "Email Input", "Password Input", "Submit Button"] };
})()
`,
    },
    {
        keywords: ["card", "cards", "grid", "column", "columns"],
        label: "Card grid",
        code: (x, y, w) => `
(async () => {
  const cols = 3;
  const gap = 24;
  const cardW = Math.floor((${w} - gap * (cols + 1)) / cols);
  const cardH = 280;

  const results = [];
  for (let i = 0; i < cols; i++) {
    const cx = ${x} + gap + i * (cardW + gap);

    const card = logos.createFrame();
    card.name = "Card " + (i + 1);
    card.x = cx; card.y = ${y} + gap;
    card.width = cardW; card.height = cardH;
    card.fills = [{ fillType: 'solid', fillColor: '#1e1e2e', fillOpacity: 1 }];
    card.borderRadius = 12;

    const img = logos.createRect();
    img.name = "Image " + (i + 1);
    img.x = cx; img.y = ${y} + gap;
    img.width = cardW; img.height = 140;
    img.fills = [{ fillType: 'solid', fillColor: '#313244', fillOpacity: 1 }];
    img.borderRadius = 12;

    const cardTitle = logos.createText("Card Title " + (i + 1));
    cardTitle.name = "Card Title " + (i + 1);
    cardTitle.x = cx + 16; cardTitle.y = ${y} + gap + 152;
    cardTitle.width = cardW - 32; cardTitle.height = 24;
    cardTitle.fontSize = 16; cardTitle.fontWeight = "600";
    cardTitle.fills = [{ fillType: 'solid', fillColor: '#cdd6f4', fillOpacity: 1 }];

    const body = logos.createText("Short description of the card content.");
    body.name = "Body " + (i + 1);
    body.x = cx + 16; body.y = ${y} + gap + 180;
    body.width = cardW - 32; body.height = 40;
    body.fontSize = 13;
    body.fills = [{ fillType: 'solid', fillColor: '#a6adc8', fillOpacity: 1 }];

    const cta = logos.createRect();
    cta.name = "CTA " + (i + 1);
    cta.x = cx + 16; cta.y = ${y} + gap + 232;
    cta.width = 96; cta.height = 32;
    cta.fills = [{ fillType: 'solid', fillColor: '#89b4fa', fillOpacity: 1 }];
    cta.borderRadius = 6;

    results.push("Card " + (i + 1));
  }
  return { created: results };
})()
`,
    },
    {
        keywords: ["nav", "navbar", "navigation", "header", "menu"],
        label: "Navigation bar",
        code: (x, y, w) => `
(async () => {
  const nav = logos.createFrame();
  nav.name = "Navigation Bar";
  nav.x = ${x}; nav.y = ${y};
  nav.width = ${w}; nav.height = 64;
  nav.fills = [{ fillType: 'solid', fillColor: '#181825', fillOpacity: 1 }];

  const logo = logos.createText("Logos");
  logo.name = "Logo"; logo.x = ${x} + 24; logo.y = ${y} + 18;
  logo.width = 80; logo.height = 28;
  logo.fontSize = 20; logo.fontWeight = "700";
  logo.fills = [{ fillType: 'solid', fillColor: '#89b4fa', fillOpacity: 1 }];

  const items = ["Features", "Pricing", "Docs", "Sign in"];
  const itemW = 80;
  const startX = ${x} + ${w} - items.length * (itemW + 8) - 24;
  items.forEach((label, i) => {
    const item = logos.createText(label);
    item.name = label;
    item.x = startX + i * (itemW + 8); item.y = ${y} + 20;
    item.width = itemW; item.height = 24;
    item.fontSize = 14; item.textAlign = "center";
    item.fills = [{ fillType: 'solid', fillColor: '#cdd6f4', fillOpacity: 1 }];
  });

  return { created: ["Navigation Bar", ...items] };
})()
`,
    },
    {
        keywords: ["form", "input", "field", "fields", "contact"],
        label: "Contact form",
        code: (x, y, w) => `
(async () => {
  const fw = Math.min(${w}, 560);
  const cx = ${x} + (${w} - fw) / 2;
  const fields = [
    { label: "Name", row: 0 },
    { label: "Email", row: 1 },
    { label: "Message", row: 2, tall: true },
  ];

  const frame = logos.createFrame();
  frame.name = "Contact Form";
  frame.x = cx; frame.y = ${y};
  frame.width = fw; frame.height = 480;
  frame.fills = [{ fillType: 'solid', fillColor: '#1e1e2e', fillOpacity: 1 }];
  frame.borderRadius = 12;

  const title = logos.createText("Get in touch");
  title.name = "Title"; title.x = cx + 32; title.y = ${y} + 32;
  title.width = fw - 64; title.height = 40;
  title.fontSize = 26; title.fontWeight = "700";
  title.fills = [{ fillType: 'solid', fillColor: '#cdd6f4', fillOpacity: 1 }];

  let offsetY = ${y} + 96;
  const created = ["Contact Form"];
  for (const f of fields) {
    const lbl = logos.createText(f.label);
    lbl.name = f.label + " Label"; lbl.x = cx + 32; lbl.y = offsetY;
    lbl.width = fw - 64; lbl.height = 18; lbl.fontSize = 13;
    lbl.fills = [{ fillType: 'solid', fillColor: '#a6adc8', fillOpacity: 1 }];

    const inp = logos.createRect();
    inp.name = f.label + " Input"; inp.x = cx + 32; inp.y = offsetY + 22;
    inp.width = fw - 64; inp.height = f.tall ? 100 : 44;
    inp.fills = [{ fillType: 'solid', fillColor: '#313244', fillOpacity: 1 }];
    inp.borderRadius = 6;

    offsetY += (f.tall ? 100 : 44) + 32;
    created.push(f.label + " Input");
  }

  const btn = logos.createRect();
  btn.name = "Send Button"; btn.x = cx + 32; btn.y = offsetY;
  btn.width = fw - 64; btn.height = 48;
  btn.fills = [{ fillType: 'solid', fillColor: '#89b4fa', fillOpacity: 1 }];
  btn.borderRadius = 8;
  created.push("Send Button");

  return { created };
})()
`,
    },
];

// ─────────────────────────────────────────────────────────────────────────────
// Tool
// ─────────────────────────────────────────────────────────────────────────────

export class GenerateLayoutTool extends Tool<GenerateLayoutArgs> {
    constructor(mcpServer: LogosMcpServer) {
        super(mcpServer, GenerateLayoutArgs.schema);
    }

    public getToolName(): string {
        return "generate_layout";
    }

    public getToolDescription(): string {
        return (
            "Generates a UI layout on the Logos canvas from a natural language description.\n" +
            "Supported layout types: hero section, login/auth form, card grid, navigation bar, contact form.\n" +
            "Parameters:\n" +
            "  prompt  — description of the desired layout (required)\n" +
            "  x, y    — top-left position of the generated frame (default: 0, 0)\n" +
            "  width   — frame width in pixels (default: 800)\n" +
            "Returns a summary of the shapes created."
        );
    }

    protected async executeCore(args: GenerateLayoutArgs): Promise<ToolResponse> {
        const x = args.x ?? 0;
        const y = args.y ?? 0;
        const width = args.width ?? 800;
        const prompt = args.prompt.toLowerCase();

        // Find matching template
        const matched = TEMPLATES.find((t) =>
            t.keywords.some((kw) => prompt.includes(kw))
        );

        if (!matched) {
            // Fallback: generate a generic frame with a label
            const fallbackCode = `
(async () => {
  const frame = logos.createFrame();
  frame.name = "Generated Frame";
  frame.x = ${x}; frame.y = ${y};
  frame.width = ${width}; frame.height = 400;
  frame.fills = [{ fillType: 'solid', fillColor: '#1e1e2e', fillOpacity: 1 }];
  frame.borderRadius = 8;

  const label = logos.createText(${JSON.stringify(args.prompt)});
  label.name = "Prompt Label";
  label.x = ${x} + 32; label.y = ${y} + 32;
  label.width = ${width} - 64; label.height = 40;
  label.fontSize = 18; label.fontWeight = "600";
  label.fills = [{ fillType: 'solid', fillColor: '#a6adc8', fillOpacity: 1 }];

  return { created: ["Generated Frame", "Prompt Label"] };
})()
`;
            const task = new ExecuteCodePluginTask({ code: fallbackCode });
            const result = await this.mcpServer.pluginBridge.executePluginTask(task);
            return new TextResponse(
                `No specific template matched. Created a generic frame.\n\n` +
                `Result: ${JSON.stringify(result.data, null, 2)}`
            );
        }

        const code = matched.code(x, y, width);
        const task = new ExecuteCodePluginTask({ code });
        const result = await this.mcpServer.pluginBridge.executePluginTask(task);

        return new TextResponse(
            `Generated layout: "${matched.label}"\n\n` +
            `Result: ${JSON.stringify(result.data, null, 2)}`
        );
    }
}
