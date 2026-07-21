import { describe, it, expect } from 'vitest';
import type { MaterialPreset } from '../material';
import type { ConsoleEntry, ConsoleDirection } from '../console';
import type { MacroDefinition } from '../macro';
import type { PaletteColor } from '../palette';

describe('types/macro', () => {
  it('MacroDefinition has correct shape', () => {
    const macro: MacroDefinition = {
      id: 'm1',
      name: 'Home and zero',
      description: 'Homes the machine and zeros axes',
      commands: ['$H', 'G92 X0 Y0'],
      hotkey: 'ctrl+shift+h',
      show_in_toolbar: true,
    };
    expect(macro.id).toBe('m1');
    expect(macro.description).toContain('Homes');
  });
});

describe('types/palette', () => {
  it('PaletteColor has correct shape', () => {
    // is_tool_layer is non-optional — every palette entry sets it explicitly.
    const color: PaletteColor = { hex: '#FF0000', name: 'Red', is_tool_layer: false };
    expect(color.hex).toBe('#FF0000');
    expect(color.is_tool_layer).toBe(false);

    const tool: PaletteColor = { hex: '#000000', name: 'T1', is_tool_layer: true };
    expect(tool.is_tool_layer).toBe(true);
  });
});

describe('types/material', () => {
  it('MaterialPreset has correct shape', () => {
    // notes and category are non-optional (backend `#[serde(default)]` → empty string).
    const preset: MaterialPreset = {
      id: 'mat-1',
      name: '3mm Plywood',
      material: 'Wood',
      thickness_mm: 3.0,
      operation: 'cut',
      speed_mm_min: 1000,
      power_percent: 80,
      passes: 2,
      notes: 'Use exhaust fan',
      category: '',
    };
    expect(preset.id).toBe('mat-1');
    expect(preset.operation).toBe('cut');
    expect(preset.speed_mm_min).toBe(1000);
  });
});

describe('types/console', () => {
  it('ConsoleEntry has correct shape', () => {
    // no `is_error` field — backend doesn't emit it. UI derives
    // error state from `content` (e.g. messages starting with "error:").
    const entry: ConsoleEntry = {
      timestamp: '2026-03-19T12:00:00Z',
      direction: 'sent',
      content: 'G0 X10 Y10',
    };
    expect(entry.direction).toBe('sent');

    const errorLine: ConsoleEntry = {
      timestamp: '2026-03-19T12:00:01Z',
      direction: 'received',
      content: 'error:20',
    };
    expect(errorLine.content.startsWith('error:')).toBe(true);
  });

  it('ConsoleDirection accepts both values', () => {
    const dirs: ConsoleDirection[] = ['sent', 'received'];
    expect(dirs).toHaveLength(2);
  });
});
