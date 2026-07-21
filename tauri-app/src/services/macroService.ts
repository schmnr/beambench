import { invoke } from '@tauri-apps/api/core';
import type { MacroDefinition } from '../types/macro';

export const macroService = {
  async getMacros(): Promise<MacroDefinition[]> {
    return invoke<MacroDefinition[]>('get_macros');
  },

  // backend `save_macro` returns `()`, not the macro.
  async saveMacro(macroDef: MacroDefinition): Promise<void> {
    return invoke<void>('save_macro', { macroDef });
  },

  async deleteMacro(macroId: string): Promise<void> {
    return invoke<void>('delete_macro', { macroId });
  },

  async runMacro(macroId: string): Promise<void> {
    return invoke<void>('run_macro', { macroId });
  },
};
