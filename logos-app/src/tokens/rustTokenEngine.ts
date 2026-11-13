import type { ResolveRequest, ResolveResult, TokenSnapshot, UserIntent, DocumentContext, ProposedPlan, ValidationResult } from "./contract";

interface TokenWasmExports extends WebAssembly.Exports {
  memory: WebAssembly.Memory;
  logos_tokens_alloc(length: number): number;
  logos_tokens_free_input(pointer: number, length: number): void;
  logos_tokens_output_ptr(): number;
  logos_tokens_free_output(): void;
  logos_tokens_resolve_json(pointer: number, length: number): number;
  logos_tokens_validate_plan_json(pointer: number, length: number): number;
}

export interface RustTokenEngine {
  resolve(snapshot: TokenSnapshot, request: ResolveRequest): ResolveResult;
  validatePlan(snapshot: TokenSnapshot, intent: UserIntent, context: DocumentContext, plan: ProposedPlan): ValidationResult;
}

export async function loadRustTokenEngine(wasmUrl: string): Promise<RustTokenEngine> {
  const response = await fetch(wasmUrl);
  if (!response.ok) throw new Error(`Rust token engine unavailable (${response.status})`);
  const { instance } = await WebAssembly.instantiate(await response.arrayBuffer(), {});
  const wasm = instance.exports as TokenWasmExports;
  const required: (keyof TokenWasmExports)[] = ["memory", "logos_tokens_alloc", "logos_tokens_free_input", "logos_tokens_output_ptr", "logos_tokens_free_output", "logos_tokens_resolve_json", "logos_tokens_validate_plan_json"];
  if (required.some((name) => !(name in wasm))) throw new Error("Rust token engine exports are incomplete");

  const invoke = <T>(run: (pointer: number, length: number) => number, input: unknown): T => {
    const bytes = new TextEncoder().encode(JSON.stringify(input));
    const pointer = wasm.logos_tokens_alloc(bytes.length);
    if (pointer === 0) throw new Error("Rust token engine input allocation failed");
    new Uint8Array(wasm.memory.buffer, pointer, bytes.length).set(bytes);
    try {
      const outputLength = run(pointer, bytes.length);
      if (outputLength === 0) throw new Error("Rust token engine returned an empty response");
      const outputPointer = wasm.logos_tokens_output_ptr();
      const output = new TextDecoder().decode(new Uint8Array(wasm.memory.buffer, outputPointer, outputLength));
      const parsed = JSON.parse(output) as T | { status: "ERROR"; code: string };
      if (typeof parsed === "object" && parsed !== null && "status" in parsed && parsed.status === "ERROR" && "code" in parsed) {
        throw new Error(`Rust token engine rejected the contract message (${parsed.code})`);
      }
      return parsed as T;
    } finally {
      wasm.logos_tokens_free_output();
      wasm.logos_tokens_free_input(pointer, bytes.length);
    }
  };

  return {
    resolve: (snapshot, request) => invoke<ResolveResult>(wasm.logos_tokens_resolve_json, { snapshot, request }),
    validatePlan: (snapshot, intent, context, plan) => invoke<ValidationResult>(wasm.logos_tokens_validate_plan_json, { snapshot, intent, context, plan }),
  };
}
