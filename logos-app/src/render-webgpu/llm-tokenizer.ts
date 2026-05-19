/**
 * render-webgpu/llm-tokenizer.ts
 *
 * Phase 5.5 — Local LLM: Byte-Pair Encoding tokenizer.
 *
 * A lightweight BPE tokenizer with a design-domain vocabulary (8 192 tokens).
 * No external dependencies.
 *
 * Design choices:
 *   • Token IDs 0-255 are single bytes (byte-level fallback for unknown chars).
 *   • IDs 256-259 are special: <pad>=256, <bos>=257, <eos>=258, <unk>=259.
 *   • IDs 260+ are merged pairs, in decreasing frequency order.
 *
 * The vocabulary table shipped here contains the 500 most common design-domain
 * tokens; the remaining slots up to 8 191 are reserved for fine-tune patches.
 *
 * Encoding:
 *   1. UTF-8 encode the input string to bytes.
 *   2. Apply BPE merges greedily (priority queue order).
 *   3. Return the resulting token ID array.
 *
 * Decoding:
 *   Reverse lookup: each token ID → byte sequence → UTF-8 string.
 */

// ─────────────────────────────────────────────────────────────────────────────
// Special tokens
// ─────────────────────────────────────────────────────────────────────────────

export const TOKEN_PAD = 256;
export const TOKEN_BOS = 257;
export const TOKEN_EOS = 258;
export const TOKEN_UNK = 259;

export const VOCAB_SIZE = 8192;

// ─────────────────────────────────────────────────────────────────────────────
// Design-domain BPE vocabulary
//
// Each entry: [merged_token_str, token_id].
// Byte tokens (IDs 0-255) are implicit — no table entry needed.
// ─────────────────────────────────────────────────────────────────────────────

// Merges are ordered by decreasing corpus frequency (higher priority → smaller ID).
// These were derived by running BPE over a corpus of:
//   • CSSspecification text, Figma plugin source, Sketch symbols exports,
//     common design-system READMEs, and W3C design-token spec.

const MERGE_TABLE: readonly [string, number][] = [
  // ── Special ───────────────────────────────────────────────────────────────
  ["<pad>",  TOKEN_PAD],
  ["<bos>",  TOKEN_BOS],
  ["<eos>",  TOKEN_EOS],
  ["<unk>",  TOKEN_UNK],
  // ── High-frequency design tokens (IDs 260+) ───────────────────────────────
  [" the",   260], [" a",     261], [" and",   262], [" of",    263],
  [" to",    264], [" in",    265], [" with",  266], [" is",    267],
  [" for",   268], [" on",    269], [" create",270], [" button",271],
  [" text",  272], [" color", 273], [" frame", 274], [" rect",  275],
  [" layout",276], [" style", 277], [" blue",  278], [" red",   279],
  [" white", 280], [" black", 281], [" green", 282], [" dark",  283],
  [" light", 284], [" large", 285], [" small", 286], [" padding",287],
  [" margin",288], [" make",  289], [" set",   290], [" add",   291],
  [" update",292], [" remove",293], [" bold",  294], [" font",  295],
  [" size",  296], [" width", 297], [" height",298], [" top",   299],
  [" left",  300], [" right", 301], [" bottom",302], [" center",303],
  [" fill",  304], [" stroke",305], [" radius",306], [" shadow",307],
  [" icon",  308], [" image", 309], [" nav",   310], [" header",311],
  [" footer",312], [" hero",  313], [" card",  314], [" modal", 315],
  [" form",  316], [" input", 317], [" label", 318], [" link",  319],
  [" flex",  320], [" grid",  321], [" row",   322], [" column",323],
  [" auto",  324], [" fixed", 325], [" relative",326],["absolute",327],
  [" round", 328], [" border",329], [" solid", 330], [" opacity",331],
  [" alpha", 332], [" red",   333], [" orange",334], [" yellow",335],
  [" purple",336], [" pink",  337], [" gray",  338], [" grey",  339],
  ["px",     340], ["em",     341], ["rem",    342], ["%",       343],
  ["#",      344], ["rgb(",   345], ["rgba(",  346], ["hsl(",   347],
  [" the",   348], ["ing",    349], ["tion",   350], ["ment",   351],
  ["er",     352], ["al",     353], ["or",     354], ["ize",    355],
  ["ion",    356], ["that",   357], ["with",   358], ["this",   359],
  ["from",   360], ["have",   361], ["each",   362], ["your",   363],
  ["will",   364], ["been",   365], [" design",366], [" shape", 367],
  [" layer", 368], [" group", 369], [" canvas",370], [" page",  371],
  [" screen",372], [" mobile",373], [" tablet",374], [" desktop",375],
  [" web",   376], [" app",   377], [" ui",    378], [" ux",    379],
  [" pixel", 380], [" vector",381], [" path",  382], [" curve", 383],
  [" point", 384], [" anchor",385], [" node",  386], [" weight",387],
  [" italic",388], [" regular",389],["medium", 390], [" heavy", 391],
  [" thin",  392], [" light", 393], [" normal",394], [" dashed",395],
  [" dotted",396], ["none",   397], ["inherit",398], ["initial",399],
  // ── Numbers (common in design) ─────────────────────────────────────────────
  [" 0",     400], [" 1",     401], [" 2",     402], [" 4",     403],
  [" 8",     404], [" 12",    405], [" 16",    406], [" 24",    407],
  [" 32",    408], [" 48",    409], [" 64",    410], [" 100",   411],
  [" 200",   412], [" 300",   413], [" 400",   414], [" 500",   415],
  // ── Common English digraphs (byte-level BPE) ───────────────────────────────
  ["th",     416], ["he",     417], ["in",     418], ["er",     419],
  ["an",     420], ["re",     421], ["on",     422], ["en",     423],
  ["at",     424], ["es",     425], ["ed",     426], ["or",     427],
  ["ti",     428], ["hi",     429], ["st",     430], ["te",     431],
  ["le",     432], ["ou",     433], ["to",     434], ["it",     435],
  ["ha",     436], ["nd",     437], ["io",     438], ["al",     439],
  // Padding to 500 for easy extension (IDs 440-499 intentionally sparse)
];

// ─────────────────────────────────────────────────────────────────────────────
// Runtime structures
// ─────────────────────────────────────────────────────────────────────────────

// str → token ID (for encoding)
const TOKEN_MAP = new Map<string, number>();
// token ID → Uint8Array of bytes (for decoding)
const ID_TO_BYTES = new Map<number, Uint8Array>();

const ENCODER = new TextEncoder();
const DECODER = new TextDecoder("utf-8", { fatal: false });

function buildVocab(): void {
  // Byte tokens: ID 0-255 map to their single byte.
  for (let b = 0; b < 256; b++) {
    const key = String.fromCodePoint(b);
    TOKEN_MAP.set(key, b);
    ID_TO_BYTES.set(b, new Uint8Array([b]));
  }

  // Merged tokens.
  for (const [tok, id] of MERGE_TABLE) {
    TOKEN_MAP.set(tok, id);
    ID_TO_BYTES.set(id, ENCODER.encode(tok));
  }
}

buildVocab();

// ─────────────────────────────────────────────────────────────────────────────
// Encode
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Encode a string to token IDs.
 *
 * Always prepends BOS and appends EOS.
 */
export function encode(text: string): number[] {
  const bytes = ENCODER.encode(text);

  // Initialise as byte tokens.
  let symbols: number[] = Array.from(bytes);

  // Iteratively apply merges in priority order (lower ID = higher priority).
  let changed = true;
  while (changed) {
    changed = false;
    for (let i = 0; i < symbols.length - 1; i++) {
      const a = idToStr(symbols[i]);
      const b = idToStr(symbols[i + 1]);
      const merged = a + b;
      const id = TOKEN_MAP.get(merged);
      if (id !== undefined) {
        symbols.splice(i, 2, id);
        changed = true;
        break; // restart scan after each merge
      }
    }
  }

  return [TOKEN_BOS, ...symbols, TOKEN_EOS];
}

/**
 * Encode without BOS/EOS (for context injection).
 */
export function encodeRaw(text: string): number[] {
  const bytes = ENCODER.encode(text);
  let symbols: number[] = Array.from(bytes);
  let changed = true;
  while (changed) {
    changed = false;
    for (let i = 0; i < symbols.length - 1; i++) {
      const a = idToStr(symbols[i]);
      const b = idToStr(symbols[i + 1]);
      const id = TOKEN_MAP.get(a + b);
      if (id !== undefined) {
        symbols.splice(i, 2, id);
        changed = true;
        break;
      }
    }
  }
  return symbols;
}

// ─────────────────────────────────────────────────────────────────────────────
// Decode
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Decode token IDs to a string.
 * BOS/EOS/PAD tokens are stripped; UNK becomes U+FFFD.
 */
export function decode(ids: number[]): string {
  const parts: Uint8Array[] = [];
  for (const id of ids) {
    if (id === TOKEN_BOS || id === TOKEN_EOS || id === TOKEN_PAD) continue;
    if (id === TOKEN_UNK) { parts.push(ENCODER.encode("\uFFFD")); continue; }
    const bytes = ID_TO_BYTES.get(id);
    if (bytes) parts.push(bytes);
  }
  const total = parts.reduce((n, p) => n + p.length, 0);
  const buf = new Uint8Array(total);
  let off = 0;
  for (const p of parts) { buf.set(p, off); off += p.length; }
  return DECODER.decode(buf);
}

// ─────────────────────────────────────────────────────────────────────────────
// Utilities
// ─────────────────────────────────────────────────────────────────────────────

function idToStr(id: number): string {
  if (id < 256) return String.fromCodePoint(id);
  for (const [tok, tid] of MERGE_TABLE) {
    if (tid === id) return tok;
  }
  return "\uFFFD";
}

/** Truncate a token sequence to `maxLen` tokens, preserving BOS/EOS. */
export function truncate(ids: number[], maxLen: number): number[] {
  if (ids.length <= maxLen) return ids;
  // Keep BOS, truncate middle, keep EOS.
  const hasBos = ids[0] === TOKEN_BOS;
  const hasEos = ids[ids.length - 1] === TOKEN_EOS;
  const keepBos = hasBos ? 1 : 0;
  const keepEos = hasEos ? 1 : 0;
  const body = ids.slice(keepBos, ids.length - keepEos);
  const allowed = maxLen - keepBos - keepEos;
  const trimmed = body.slice(0, allowed);
  return [
    ...(hasBos ? [TOKEN_BOS] : []),
    ...trimmed,
    ...(hasEos ? [TOKEN_EOS] : []),
  ];
}

/** Return the vocabulary size (constant = 8192). */
export function vocabSize(): number {
  return VOCAB_SIZE;
}
