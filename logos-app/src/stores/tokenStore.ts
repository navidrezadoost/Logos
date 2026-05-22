/**
 * stores/tokenStore.ts
 *
 * Phase IM1 — Zustand store for the Logos token runtime.
 *
 * Holds:
 *   sets    — TokenSet per Figma collection (or manually created)
 *   themes  — TokenTheme per Figma mode (or manually created)
 *   active  — the currently active theme IDs (one per collection group)
 *
 * After import the resolved token map is lazily computed so consumers can
 * call resolveToken("Brand/Primary/500") and get the final value accounting
 * for the active theme and alias chains.
 */

import { create } from "zustand";
import { persist } from "zustand/middleware";
import type { LogosTokenSet, LogosTokenTheme, LogosToken } from "../migration/figma/figma-token-converter";

// Re-export so UI code doesn't need to reach into migration/
export type { LogosToken, LogosTokenSet, LogosTokenTheme };

// ─── State ───────────────────────────────────────────────────────────────────

interface TokenState {
  /** All token sets in the current document */
  sets: LogosTokenSet[];
  /** All themes (mode overrides) */
  themes: LogosTokenTheme[];
  /**
   * Active theme IDs keyed by group (collection name).
   * When a group has no active theme, the base set values are used.
   */
  activeThemeIds: Record<string, string>;

  // ── Actions ─────────────────────────────────────────────────────────────

  /**
   * Load the result of a Figma import (or any other source).
   * Merges into existing token data — tokens from different imports coexist.
   */
  loadImport: (sets: LogosTokenSet[], themes: LogosTokenTheme[]) => void;

  /** Replace all token data (destructive — used for "open file") */
  replaceAll: (sets: LogosTokenSet[], themes: LogosTokenTheme[]) => void;

  /** Activate a theme by ID within its group */
  activateTheme: (themeId: string, group: string) => void;

  /** Deactivate (return to base values) for a collection group */
  deactivateTheme: (group: string) => void;

  /** Clear all token data */
  clear: () => void;

  // ── Derived helpers ──────────────────────────────────────────────────────

  /**
   * Resolve a token name to its final value, following alias chains and
   * respecting the active theme overrides.
   *
   * Returns undefined if the token cannot be found or the alias chain
   * contains a cycle or broken reference.
   */
  resolveToken: (name: string, visited?: Set<string>) => string | undefined;

  /** Get the flat token list across all sets */
  allTokens: () => LogosToken[];
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

const ALIAS_RE = /^\{(.+)\}$/;

function resolveAlias(
  value: string,
  lookup: (name: string) => string | undefined,
  visited: Set<string>
): string | undefined {
  const match = ALIAS_RE.exec(value);
  if (!match) return value; // not an alias — it's a raw value

  const targetName = match[1];
  if (visited.has(targetName)) {
    console.warn(`[tokenStore] Circular alias detected: ${targetName}`);
    return undefined;
  }

  visited.add(targetName);
  const targetValue = lookup(targetName);
  if (targetValue === undefined) return undefined;

  return resolveAlias(targetValue, lookup, visited);
}

// ─── Store ───────────────────────────────────────────────────────────────────

export const useTokenStore = create<TokenState>()(
  persist(
    (set, get) => ({
      sets: [],
      themes: [],
      activeThemeIds: {},

      loadImport(newSets, newThemes) {
        set((state) => ({
          sets:   [...state.sets,   ...newSets],
          themes: [...state.themes, ...newThemes],
        }));
      },

      replaceAll(newSets, newThemes) {
        set({ sets: newSets, themes: newThemes, activeThemeIds: {} });
      },

      activateTheme(themeId, group) {
        set((state) => ({
          activeThemeIds: { ...state.activeThemeIds, [group]: themeId },
        }));
      },

      deactivateTheme(group) {
        set((state) => {
          const next = { ...state.activeThemeIds };
          delete next[group];
          return { activeThemeIds: next };
        });
      },

      clear() {
        set({ sets: [], themes: [], activeThemeIds: {} });
      },

      allTokens() {
        return get().sets.flatMap((s) => s.tokens);
      },

      resolveToken(name, visited = new Set<string>()) {
        const state = get();

        // Build a value lookup: first check active theme overrides,
        // then fall back to base token set values.
        const lookup = (tokenName: string): string | undefined => {
          // Active theme overrides take priority
          for (const theme of state.themes) {
            const activeId = state.activeThemeIds[theme.group];
            if (activeId === theme.id) {
              const override = theme.overrides[tokenName];
              if (override !== undefined) return override;
            }
          }

          // Fall back to base set
          for (const tokenSet of state.sets) {
            const token = tokenSet.tokens.find((t) => t.name === tokenName);
            if (token) return token.value;
          }

          return undefined;
        };

        const rawValue = lookup(name);
        if (rawValue === undefined) return undefined;

        visited.add(name);
        return resolveAlias(rawValue, lookup, visited);
      },
    }),
    {
      name: "logos-token-store",
      // Persist tokens across reloads so imported tokens survive page refresh.
      // Sets and themes serialize cleanly as plain JSON objects.
    }
  )
);
