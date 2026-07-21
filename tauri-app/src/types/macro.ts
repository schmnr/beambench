/** Macro definition types for user-defined G-code macros — matches Rust MacroDefinition. */

export interface MacroDefinition {
  id: string;
  name: string;
  description: string;
  commands: string[];
  hotkey?: string | null;
  show_in_toolbar?: boolean;
}
