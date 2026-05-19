/**
 * templates/index.ts
 *
 * P4.7 — Template Library (Phase 4)
 *
 * All templates are bundled at build time: no network request, fully offline.
 * Each TemplateData carries the shapes array that will be re-UUIDed on insert.
 *
 * Template IDs use static strings here; templateStore.ts remaps them to fresh
 * crypto.randomUUID() values so inserted templates never collide.
 */

import { type Shape, IDENTITY_TRANSFORM } from "../types/shapes";

// ─────────────────────────────────────────────────────────────────────────────
// Category & Template metadata
// ─────────────────────────────────────────────────────────────────────────────

export type TemplateCategory =
    | "Web"
    | "Mobile"
    | "Social Media"
    | "Presentation"
    | "Wireframe";

export interface TemplateMeta {
    id: string;
    name: string;
    category: TemplateCategory;
    description: string;
    /** SVG data URI used as the thumbnail (512×320 viewBox). */
    thumbnailSvg: string;
}

export interface TemplateData extends TemplateMeta {
    /**
     * The shapes for this template. IDs are placeholder strings and will be
     * remapped to new UUIDs by templateStore.insertTemplate().
     * parentId fields reference other shapes by their placeholder IDs;
     * the store remaps those too.
     */
    shapes: Shape[];
}

// ─────────────────────────────────────────────────────────────────────────────
// Shape factory helpers
// ─────────────────────────────────────────────────────────────────────────────

function r(
    id: string,
    name: string,
    x: number, y: number, w: number, h: number,
    color: string,
    parentId: string | null = null
): Shape {
    return {
        id, type: "rect", name,
        bounds: { x, y, w, h },
        transform: IDENTITY_TRANSFORM,
        rotation: 0,
        fills: [{ type: "solid", color, opacity: 1 }],
        opacity: 1, hidden: false, locked: false,
        parentId, children: [],
    };
}

// Thumbnail SVG builder — generates a minimal layout sketch (512×320 canvas)
function svgUri(elements: string): string {
    const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 320" width="512" height="320">${elements}</svg>`;
    return `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`;
}

// ─────────────────────────────────────────────────────────────────────────────
// ─── WEB TEMPLATES ───────────────────────────────────────────────────────────
// ─────────────────────────────────────────────────────────────────────────────

// 1. Hero Section
const HERO: TemplateData = {
    id: "web-hero-section",
    name: "Hero Section",
    category: "Web",
    description: "Full-width hero with heading, subheading, and two CTA buttons.",
    thumbnailSvg: svgUri(`
        <rect width="512" height="320" fill="#1e1e2e"/>
        <rect x="20" y="30" width="360" height="36" rx="4" fill="#4a5568"/>
        <rect x="20" y="82" width="260" height="18" rx="3" fill="#313244"/>
        <rect x="20" y="116" width="140" height="42" rx="6" fill="#89b4fa"/>
        <rect x="174" y="116" width="120" height="42" rx="6" fill="#313244"/>
        <rect x="20" y="190" width="472" height="110" rx="6" fill="#181825"/>
    `),
    shapes: [
        r("h-bg",      "Hero / Background",    0,   0, 800, 480, "#1e1e2e"),
        r("h-head",    "Hero / Heading",       60, 120, 680,  64, "#4a5568"),
        r("h-sub",     "Hero / Subheading",    60, 200, 500,  36, "#313244"),
        r("h-cta1",    "Hero / Primary CTA",   60, 280, 180,  52, "#89b4fa"),
        r("h-cta2",    "Hero / Secondary CTA", 256, 280, 160, 52, "#313244"),
        r("h-img",     "Hero / Visual",          0, 380, 800, 240, "#181825"),
    ],
};

// 2. Navbar + Content
const NAVBAR_CONTENT: TemplateData = {
    id: "web-navbar-content",
    name: "Navbar + Content",
    category: "Web",
    description: "Navigation bar with logo and links, plus a content area below.",
    thumbnailSvg: svgUri(`
        <rect width="512" height="320" fill="#1e1e2e"/>
        <rect x="0" y="0" width="512" height="56" fill="#181825"/>
        <rect x="20" y="16" width="80" height="24" rx="4" fill="#89b4fa"/>
        <rect x="360" y="18" width="48" height="20" rx="3" fill="#313244"/>
        <rect x="420" y="18" width="48" height="20" rx="3" fill="#313244"/>
        <rect x="20" y="80" width="220" height="28" rx="4" fill="#4a5568"/>
        <rect x="20" y="120" width="472" height="14" rx="3" fill="#313244"/>
        <rect x="20" y="142" width="380" height="14" rx="3" fill="#313244"/>
        <rect x="20" y="164" width="420" height="14" rx="3" fill="#313244"/>
        <rect x="20" y="200" width="140" height="44" rx="6" fill="#89b4fa"/>
    `),
    shapes: [
        r("nc-navbar", "Navbar / Background",   0,   0, 1280,  72, "#181825"),
        r("nc-logo",   "Navbar / Logo",         24,  22,  120,  28, "#89b4fa"),
        r("nc-link1",  "Navbar / Link 1",      900,  24,   80,  24, "#313244"),
        r("nc-link2",  "Navbar / Link 2",      996,  24,   80,  24, "#313244"),
        r("nc-link3",  "Navbar / Link 3",     1092,  24,   80,  24, "#313244"),
        r("nc-head",   "Content / Heading",     60, 120, 680,  56, "#4a5568"),
        r("nc-body1",  "Content / Body 1",      60, 196, 760,  18, "#313244"),
        r("nc-body2",  "Content / Body 2",      60, 224, 620,  18, "#313244"),
        r("nc-body3",  "Content / Body 3",      60, 252, 700,  18, "#313244"),
        r("nc-cta",    "Content / CTA",         60, 300, 180,  52, "#89b4fa"),
    ],
};

// 3. Footer
const FOOTER: TemplateData = {
    id: "web-footer",
    name: "Footer",
    category: "Web",
    description: "Four-column footer with logo, navigation links, and copyright.",
    thumbnailSvg: svgUri(`
        <rect width="512" height="320" fill="#1e1e2e"/>
        <rect x="0" y="0" width="512" height="256" fill="#181825"/>
        <rect x="20" y="20" width="80" height="20" rx="3" fill="#89b4fa"/>
        <rect x="20" y="50" width="100" height="12" rx="2" fill="#45475a"/>
        <rect x="20" y="68" width="80" height="12" rx="2" fill="#45475a"/>
        <rect x="168" y="20" width="60" height="14" rx="2" fill="#a6adc8"/>
        <rect x="168" y="44" width="72" height="11" rx="2" fill="#45475a"/>
        <rect x="168" y="62" width="60" height="11" rx="2" fill="#45475a"/>
        <rect x="316" y="20" width="60" height="14" rx="2" fill="#a6adc8"/>
        <rect x="316" y="44" width="72" height="11" rx="2" fill="#45475a"/>
        <rect x="464" y="20" width="28" height="28" rx="14" fill="#313244"/>
        <rect x="0" y="260" width="512" height="60" fill="#11111b"/>
        <rect x="20" y="278" width="160" height="10" rx="2" fill="#45475a"/>
    `),
    shapes: [
        r("ft-bg",       "Footer / Background",      0,    0, 1280, 320, "#181825"),
        r("ft-logo",     "Footer / Logo",            60,   40, 140,  32, "#89b4fa"),
        r("ft-tagline",  "Footer / Tagline",         60,   82, 200,  16, "#6c7086"),
        r("ft-col1-h",   "Column 1 / Heading",      320,   40,  80,  18, "#a6adc8"),
        r("ft-col1-l1",  "Column 1 / Link 1",       320,   68, 120,  14, "#6c7086"),
        r("ft-col1-l2",  "Column 1 / Link 2",       320,   90, 100,  14, "#6c7086"),
        r("ft-col1-l3",  "Column 1 / Link 3",       320,  112, 110,  14, "#6c7086"),
        r("ft-col2-h",   "Column 2 / Heading",      560,   40,  80,  18, "#a6adc8"),
        r("ft-col2-l1",  "Column 2 / Link 1",       560,   68, 120,  14, "#6c7086"),
        r("ft-col2-l2",  "Column 2 / Link 2",       560,   90, 100,  14, "#6c7086"),
        r("ft-col3-h",   "Column 3 / Heading",      800,   40,  80,  18, "#a6adc8"),
        r("ft-col3-l1",  "Column 3 / Link 1",       800,   68, 120,  14, "#6c7086"),
        r("ft-social1",  "Social / Icon 1",        1080,   40,  48,  48, "#313244"),
        r("ft-social2",  "Social / Icon 2",        1140,   40,  48,  48, "#313244"),
        r("ft-copyright","Footer / Copyright bar",    0,  260, 1280,  60, "#11111b"),
        r("ft-copy-txt", "Footer / Copyright text",  60,  278, 280,  14, "#45475a"),
    ],
};

// 4. Pricing Table
const PRICING: TemplateData = {
    id: "web-pricing-table",
    name: "Pricing Table",
    category: "Web",
    description: "Three-tier pricing table: Free, Pro, and Enterprise plans.",
    thumbnailSvg: svgUri(`
        <rect width="512" height="320" fill="#1e1e2e"/>
        <rect x="20" y="20" width="140" height="270" rx="8" fill="#181825"/>
        <rect x="174" y="10" width="160" height="290" rx="8" fill="#1a2744"/>
        <rect x="344" y="20" width="148" height="270" rx="8" fill="#181825"/>
        <rect x="30" y="30" width="80" height="14" rx="2" fill="#a6adc8"/>
        <rect x="30" y="54" width="60" height="22" rx="3" fill="#cdd6f4"/>
        <rect x="30" y="90" width="100" height="10" rx="2" fill="#45475a"/>
        <rect x="30" y="108" width="90" height="10" rx="2" fill="#45475a"/>
        <rect x="30" y="240" width="120" height="36" rx="6" fill="#313244"/>
        <rect x="184" y="20" width="80" height="14" rx="2" fill="#89b4fa"/>
        <rect x="184" y="44" width="70" height="22" rx="3" fill="#cdd6f4"/>
        <rect x="184" y="250" width="140" height="36" rx="6" fill="#89b4fa"/>
        <rect x="354" y="30" width="80" height="14" rx="2" fill="#a6adc8"/>
        <rect x="354" y="54" width="60" height="22" rx="3" fill="#cdd6f4"/>
        <rect x="354" y="240" width="128" height="36" rx="6" fill="#313244"/>
    `),
    shapes: [
        r("pr-bg",       "Pricing / Background",     0,    0, 1280, 600, "#1e1e2e"),
        r("pr-head",     "Pricing / Heading",       60,   40, 400,  56, "#cdd6f4"),
        // Free card
        r("pr-free",     "Free / Card",             60,  140, 340, 420, "#181825"),
        r("pr-free-n",   "Free / Plan Name",        84,  164, 120,  24, "#a6adc8"),
        r("pr-free-p",   "Free / Price",            84,  200, 120,  36, "#cdd6f4"),
        r("pr-free-d",   "Free / Description",      84,  250, 280,  16, "#6c7086"),
        r("pr-free-f1",  "Free / Feature 1",        84,  280, 240,  14, "#6c7086"),
        r("pr-free-f2",  "Free / Feature 2",        84,  302, 220,  14, "#6c7086"),
        r("pr-free-cta", "Free / CTA",              84,  500, 292,  48, "#313244"),
        // Pro card (highlighted)
        r("pr-pro",      "Pro / Card",             440,  120, 360, 460, "#1a2744"),
        r("pr-pro-n",    "Pro / Plan Name",        464,  144, 100,  24, "#89b4fa"),
        r("pr-pro-p",    "Pro / Price",            464,  180, 140,  36, "#cdd6f4"),
        r("pr-pro-d",    "Pro / Description",      464,  230, 300,  16, "#a6adc8"),
        r("pr-pro-f1",   "Pro / Feature 1",        464,  260, 280,  14, "#a6adc8"),
        r("pr-pro-f2",   "Pro / Feature 2",        464,  282, 260,  14, "#a6adc8"),
        r("pr-pro-f3",   "Pro / Feature 3",        464,  304, 280,  14, "#a6adc8"),
        r("pr-pro-cta",  "Pro / CTA",              464,  520, 312,  48, "#89b4fa"),
        // Enterprise card
        r("pr-ent",      "Enterprise / Card",      840,  140, 340, 420, "#181825"),
        r("pr-ent-n",    "Enterprise / Plan Name", 864,  164, 160,  24, "#a6adc8"),
        r("pr-ent-p",    "Enterprise / Price",     864,  200, 120,  36, "#cdd6f4"),
        r("pr-ent-cta",  "Enterprise / CTA",       864,  500, 292,  48, "#313244"),
    ],
};

// ─────────────────────────────────────────────────────────────────────────────
// ─── MOBILE TEMPLATES ────────────────────────────────────────────────────────
// ─────────────────────────────────────────────────────────────────────────────

// 5. App Onboarding
const ONBOARDING: TemplateData = {
    id: "mobile-app-onboarding",
    name: "App Onboarding",
    category: "Mobile",
    description: "Three-screen mobile onboarding flow with illustration, title, and CTA.",
    thumbnailSvg: svgUri(`
        <rect width="512" height="320" fill="#1e1e2e"/>
        <rect x="20" y="20" width="136" height="280" rx="16" fill="#181825"/>
        <rect x="40" y="40" width="96" height="96" rx="8" fill="#313244"/>
        <rect x="40" y="150" width="80" height="14" rx="3" fill="#a6adc8"/>
        <rect x="40" y="172" width="96" height="10" rx="2" fill="#45475a"/>
        <rect x="40" y="260" width="96" height="32" rx="6" fill="#89b4fa"/>
        <rect x="172" y="20" width="136" height="280" rx="16" fill="#181825"/>
        <rect x="192" y="40" width="96" height="96" rx="8" fill="#313244"/>
        <rect x="192" y="150" width="80" height="14" rx="3" fill="#a6adc8"/>
        <rect x="192" y="260" width="96" height="32" rx="6" fill="#89b4fa"/>
        <rect x="324" y="20" width="136" height="280" rx="16" fill="#181825"/>
        <rect x="344" y="40" width="96" height="96" rx="8" fill="#313244"/>
        <rect x="344" y="150" width="80" height="14" rx="3" fill="#a6adc8"/>
        <rect x="344" y="260" width="96" height="32" rx="6" fill="#f38ba8"/>
    `),
    shapes: [
        // Screen 1
        r("ob-s1",     "Screen 1 / Frame",        0,    0, 390, 844, "#181825"),
        r("ob-s1-ill", "Screen 1 / Illustration", 95,   80, 200, 200, "#313244"),
        r("ob-s1-h",   "Screen 1 / Heading",      20,  316, 350,  40, "#cdd6f4"),
        r("ob-s1-b",   "Screen 1 / Body",         20,  368, 350,  36, "#6c7086"),
        r("ob-s1-cta", "Screen 1 / Next",         20,  740, 350,  52, "#89b4fa"),
        // Screen 2
        r("ob-s2",     "Screen 2 / Frame",       430,    0, 390, 844, "#181825"),
        r("ob-s2-ill", "Screen 2 / Illustration", 525,  80, 200, 200, "#313244"),
        r("ob-s2-h",   "Screen 2 / Heading",      450, 316, 350,  40, "#cdd6f4"),
        r("ob-s2-cta", "Screen 2 / Next",         450, 740, 350,  52, "#89b4fa"),
        // Screen 3
        r("ob-s3",     "Screen 3 / Frame",       860,    0, 390, 844, "#181825"),
        r("ob-s3-ill", "Screen 3 / Illustration", 955,  80, 200, 200, "#cba6f7"),
        r("ob-s3-h",   "Screen 3 / Heading",      880, 316, 350,  40, "#cdd6f4"),
        r("ob-s3-cta", "Screen 3 / Get Started",  880, 740, 350,  52, "#f38ba8"),
    ],
};

// 6. Profile Screen
const PROFILE: TemplateData = {
    id: "mobile-profile-screen",
    name: "Profile Screen",
    category: "Mobile",
    description: "Mobile profile screen with avatar, stats, bio, and post grid.",
    thumbnailSvg: svgUri(`
        <rect width="512" height="320" fill="#1e1e2e"/>
        <rect x="176" y="0" width="512" height="320" fill="#181825"/>
        <rect x="216" y="16" width="60" height="60" rx="30" fill="#4a5568"/>
        <rect x="220" y="86" width="52" height="10" rx="2" fill="#a6adc8"/>
        <rect x="214" y="104" width="64" height="8" rx="2" fill="#45475a"/>
        <rect x="228" y="120" width="36" height="20" rx="4" fill="#313244"/>
        <rect x="284" y="120" width="36" height="20" rx="4" fill="#313244"/>
        <rect x="340" y="120" width="36" height="20" rx="4" fill="#313244"/>
        <rect x="200" y="150" width="220" height="34" rx="6" fill="#89b4fa"/>
        <rect x="200" y="194" width="92" height="80" rx="4" fill="#313244"/>
        <rect x="300" y="194" width="92" height="80" rx="4" fill="#313244"/>
        <rect x="400" y="194" width="92" height="80" rx="4" fill="#313244"/>
    `),
    shapes: [
        r("pf-bg",     "Profile / Background",    0,  0, 390, 844, "#181825"),
        r("pf-avatar", "Profile / Avatar",       155, 60, 80,  80, "#4a5568"),
        r("pf-name",   "Profile / Name",          60,158, 270, 32, "#cdd6f4"),
        r("pf-handle", "Profile / Handle",        60,198, 270, 20, "#6c7086"),
        r("pf-posts",  "Stats / Posts",           20,236,  96, 48, "#313244"),
        r("pf-follow", "Stats / Followers",      127,236,  96, 48, "#313244"),
        r("pf-following","Stats / Following",    234,236,  96, 48, "#313244"),
        r("pf-edit",   "Profile / Edit Button",   20,304, 350, 48, "#89b4fa"),
        r("pf-bio",    "Profile / Bio",           20,372, 350, 56, "#6c7086"),
        r("pf-g1",     "Grid / Post 1",           20,448, 114,114, "#313244"),
        r("pf-g2",     "Grid / Post 2",          142,448, 114,114, "#313244"),
        r("pf-g3",     "Grid / Post 3",          264,448, 114,114, "#313244"),
        r("pf-g4",     "Grid / Post 4",           20,570, 114,114, "#313244"),
        r("pf-g5",     "Grid / Post 5",          142,570, 114,114, "#313244"),
        r("pf-g6",     "Grid / Post 6",          264,570, 114,114, "#313244"),
    ],
};

// ─────────────────────────────────────────────────────────────────────────────
// ─── SOCIAL MEDIA TEMPLATES ──────────────────────────────────────────────────
// ─────────────────────────────────────────────────────────────────────────────

// 7. Instagram Post
const INSTAGRAM: TemplateData = {
    id: "social-instagram-post",
    name: "Instagram Post",
    category: "Social Media",
    description: "1:1 square Instagram post with image, overlay, and headline.",
    thumbnailSvg: svgUri(`
        <rect width="512" height="320" fill="#1e1e2e"/>
        <rect x="96" y="10" width="300" height="300" rx="8" fill="#181825"/>
        <rect x="96" y="10" width="300" height="300" rx="8" fill="#313244"/>
        <rect x="96" y="210" width="300" height="100" rx="0" fill="rgba(30,30,46,0.8)" opacity="0.8"/>
        <rect x="112" y="222" width="200" height="20" rx="3" fill="#cdd6f4"/>
        <rect x="112" y="250" width="140" height="12" rx="2" fill="#a6adc8"/>
        <rect x="96" y="10" width="60" height="22" rx="4" fill="#89b4fa"/>
    `),
    shapes: [
        r("ig-bg",      "Post / Background",      0,  0, 1080, 1080, "#181825"),
        r("ig-img",     "Post / Image",            0,  0, 1080, 1080, "#313244"),
        r("ig-overlay", "Post / Gradient Overlay", 0,680, 1080,  400, "#11111b"),
        r("ig-head",    "Post / Headline",        60,720,  960,   64, "#cdd6f4"),
        r("ig-sub",     "Post / Caption",         60,804,  700,   36, "#a6adc8"),
        r("ig-badge",   "Post / Category Badge",  60, 60,  180,   48, "#89b4fa"),
    ],
};

// 8. Twitter / X Header
const TWITTER: TemplateData = {
    id: "social-twitter-header",
    name: "Twitter Header",
    category: "Social Media",
    description: "1500×500 Twitter/X profile header with banner, avatar, and name.",
    thumbnailSvg: svgUri(`
        <rect width="512" height="320" fill="#1e1e2e"/>
        <rect x="0" y="20" width="512" height="170" fill="#181825"/>
        <rect x="0" y="20" width="512" height="170" fill="#1a2744"/>
        <rect x="20" y="160" width="72" height="72" rx="36" fill="#313244" stroke="#1e1e2e" stroke-width="4"/>
        <rect x="40" y="248" width="160" height="16" rx="3" fill="#cdd6f4"/>
        <rect x="40" y="272" width="100" height="12" rx="2" fill="#6c7086"/>
        <rect x="350" y="248" width="120" height="36" rx="6" fill="#89b4fa"/>
    `),
    shapes: [
        r("tw-banner", "Header / Banner",            0,   0, 1500, 500, "#1a2744"),
        r("tw-logo",   "Header / Logo / Watermark", 60,  60, 240, 100, "#89b4fa"),
        r("tw-avatar", "Profile / Avatar",           60, 400, 134, 134, "#313244"),
        r("tw-name",   "Profile / Name",            214, 520, 400,  40, "#cdd6f4"),
        r("tw-handle", "Profile / Handle",          214, 572, 260,  28, "#6c7086"),
        r("tw-cta",    "Profile / Follow Button",  1200, 520, 260,  52, "#89b4fa"),
        r("tw-bio",    "Profile / Bio",             214, 614, 700,  36, "#a6adc8"),
    ],
};

// ─────────────────────────────────────────────────────────────────────────────
// ─── PRESENTATION TEMPLATES ──────────────────────────────────────────────────
// ─────────────────────────────────────────────────────────────────────────────

// 9. Title Slide
const TITLE_SLIDE: TemplateData = {
    id: "presentation-title-slide",
    name: "Title Slide",
    category: "Presentation",
    description: "16:9 presentation title slide with heading, subtitle, and presenter name.",
    thumbnailSvg: svgUri(`
        <rect width="512" height="320" fill="#1e1e2e"/>
        <rect x="0" y="0" width="512" height="320" fill="#1e1e2e"/>
        <rect x="0" y="220" width="512" height="6" fill="#89b4fa"/>
        <rect x="60" y="80" width="380" height="52" rx="4" fill="#4a5568"/>
        <rect x="60" y="146" width="260" height="22" rx="3" fill="#313244"/>
        <rect x="60" y="240" width="100" height="14" rx="2" fill="#45475a"/>
        <rect x="60" y="262" width="140" height="10" rx="2" fill="#45475a"/>
    `),
    shapes: [
        r("ts-bg",       "Slide / Background",      0,    0, 1920, 1080, "#1e1e2e"),
        r("ts-accent",   "Slide / Accent Bar",      0,  800, 1920,   8,  "#89b4fa"),
        r("ts-logo",     "Slide / Logo",           80,   60,  180,  60,  "#89b4fa"),
        r("ts-title",    "Slide / Title",           80,  280,  1200, 120, "#cdd6f4"),
        r("ts-subtitle", "Slide / Subtitle",        80,  424,   800,  48, "#a6adc8"),
        r("ts-name",     "Slide / Presenter Name",  80,  860,   400,  32, "#6c7086"),
        r("ts-date",     "Slide / Date",            80,  908,   280,  24, "#45475a"),
    ],
};

// 10. Bullet List Slide
const BULLET_SLIDE: TemplateData = {
    id: "presentation-bullet-slide",
    name: "Bullet List Slide",
    category: "Presentation",
    description: "16:9 content slide with section title and four bullet points.",
    thumbnailSvg: svgUri(`
        <rect width="512" height="320" fill="#1e1e2e"/>
        <rect x="0" y="0" width="160" height="320" fill="#181825"/>
        <rect x="20" y="20" width="120" height="60" rx="4" fill="#89b4fa" opacity="0.3"/>
        <rect x="20" y="100" width="80" height="12" rx="2" fill="#313244"/>
        <rect x="20" y="120" width="100" height="12" rx="2" fill="#313244"/>
        <rect x="20" y="140" width="80" height="12" rx="2" fill="#313244"/>
        <rect x="180" y="30" width="300" height="30" rx="4" fill="#4a5568"/>
        <rect x="180" y="80" width="8" height="8" rx="4" fill="#89b4fa"/>
        <rect x="196" y="78" width="232" height="12" rx="2" fill="#313244"/>
        <rect x="180" y="108" width="8" height="8" rx="4" fill="#89b4fa"/>
        <rect x="196" y="106" width="200" height="12" rx="2" fill="#313244"/>
        <rect x="180" y="136" width="8" height="8" rx="4" fill="#89b4fa"/>
        <rect x="196" y="134" width="220" height="12" rx="2" fill="#313244"/>
        <rect x="180" y="164" width="8" height="8" rx="4" fill="#89b4fa"/>
        <rect x="196" y="162" width="180" height="12" rx="2" fill="#313244"/>
    `),
    shapes: [
        r("bs-bg",      "Slide / Background",       0,    0, 1920, 1080, "#1e1e2e"),
        r("bs-sidebar", "Slide / Sidebar",           0,    0,  400, 1080, "#181825"),
        r("bs-logo",    "Slide / Logo",             40,   60,  320,  80,  "#89b4fa"),
        r("bs-nav1",    "Sidebar / Nav 1",          40,  200,  320,  20,  "#313244"),
        r("bs-nav2",    "Sidebar / Nav 2",          40,  232,  280,  20,  "#313244"),
        r("bs-nav3",    "Sidebar / Nav 3",          40,  264,  300,  20,  "#313244"),
        r("bs-title",   "Content / Section Title",  480,  80,  1360,  64,  "#cdd6f4"),
        r("bs-dot1",    "Bullet 1 / Dot",           480, 200,   20,  20,  "#89b4fa"),
        r("bs-b1",      "Bullet 1 / Text",          524, 196, 1200,  28,  "#a6adc8"),
        r("bs-dot2",    "Bullet 2 / Dot",           480, 268,   20,  20,  "#89b4fa"),
        r("bs-b2",      "Bullet 2 / Text",          524, 264, 1100,  28,  "#a6adc8"),
        r("bs-dot3",    "Bullet 3 / Dot",           480, 336,   20,  20,  "#89b4fa"),
        r("bs-b3",      "Bullet 3 / Text",          524, 332, 1150,  28,  "#a6adc8"),
        r("bs-dot4",    "Bullet 4 / Dot",           480, 404,   20,  20,  "#89b4fa"),
        r("bs-b4",      "Bullet 4 / Text",          524, 400, 1050,  28,  "#a6adc8"),
        r("bs-pgnum",   "Slide / Page Number",     1820,1020,   60,  24,  "#45475a"),
    ],
};

// ─────────────────────────────────────────────────────────────────────────────
// Master export
// ─────────────────────────────────────────────────────────────────────────────

/** All templates in display order. */
export const ALL_TEMPLATES: TemplateData[] = [
    HERO, NAVBAR_CONTENT, FOOTER, PRICING,
    ONBOARDING, PROFILE,
    INSTAGRAM, TWITTER,
    TITLE_SLIDE, BULLET_SLIDE,
];

/** Category order for the gallery tabs. */
export const CATEGORIES: TemplateCategory[] = [
    "Web", "Mobile", "Social Media", "Presentation", "Wireframe",
];

/** Get templates for a given category. */
export function getByCategory(cat: TemplateCategory): TemplateData[] {
    return ALL_TEMPLATES.filter((t) => t.category === cat);
}
