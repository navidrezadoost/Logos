export type LogosTokenType =
  | "color"
  | "number"
  | "string"
  | "boolean"
  | "spacing"
  | "dimensions"
  | "opacity";

export interface LogosToken {
  id: string;
  name: string;
  type: LogosTokenType;
  value: string;
  description: string;
}

export interface LogosTokenSet {
  id: string;
  name: string;
  description: string;
  tokens: LogosToken[];
}

export interface LogosTokenTheme {
  id: string;
  name: string;
  group: string;
  description: string;
  overrides: Record<string, string>;
}

export interface TokenConversionResult {
  sets: LogosTokenSet[];
  themes: LogosTokenTheme[];
  warnings: string[];
}
