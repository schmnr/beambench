import type { VariableTextConfig, VariableTextMode, VariableTextSource } from '../types/variableText';

export function defaultVariableTextSource(): VariableTextSource {
  return {
    csvPath: null,
    csvData: [],
    fieldDefaults: {
      _serial_padding: '1',
    },
    current: 1,
    start: 1,
    end: 1,
    advanceBy: 1,
    autoAdvance: false,
    totalCopies: 1,
  };
}

export function templateHasVariableText(text: string): boolean {
  return /\{(Serial(?::[^}]*)?|Const:[^}]+|Date(?::[^}]*)?|CSV:[^}]+|Cut:[^}]+)\}/.test(text);
}

export function defaultVariableTextConfig(
  template: string,
  partial?: Partial<VariableTextConfig>,
): VariableTextConfig {
  return {
    template,
    mode: partial?.mode ?? null,
    offset: partial?.offset ?? 0,
    source: {
      ...defaultVariableTextSource(),
      ...(partial?.source ?? {}),
    },
  };
}

export function wrapSequenceValue(value: number, start: number, end: number): number {
  const low = Math.min(start, end);
  const high = Math.max(start, end);
  const size = Math.max(1, high - low + 1);
  return low + ((((value - low) % size) + size) % size);
}

export function stepVariableTextCurrent(source: VariableTextSource, direction: 1 | -1): number {
  return wrapSequenceValue(
    (source.current ?? source.currentRow ?? 1) + direction * (source.advanceBy ?? 1),
    source.start ?? 1,
    source.end ?? source.start ?? 1,
  );
}

export function resetVariableTextCurrent(source: VariableTextSource): number {
  return source.start ?? 1;
}

export function detectVariableTextWarnings(
  template: string,
  mode: VariableTextMode | null | undefined,
): string[] {
  const warnings: string[] = [];
  const hasSerial = /\{Serial(?::[^}]*)?\}/.test(template);
  const hasDate = /\{Date(?::[^}]*)?\}/.test(template);
  const hasCsv = /\{CSV:[^}]+\}/.test(template);
  const hasCut = /\{Cut:[^}]+\}/.test(template);
  const hasConst = /\{Const:[^}]+\}/.test(template);
  const hasAny = hasSerial || hasDate || hasCsv || hasCut || hasConst;

  if (hasSerial && mode !== 'serial_number') warnings.push('Template contains {Serial} placeholders outside Serial Number mode.');
  if (hasDate && mode !== 'date_time') warnings.push('Template contains {Date:...} placeholders outside Date/Time mode.');
  if (hasCsv && mode !== 'merge_csv') warnings.push('Template contains {CSV:...} placeholders outside Merge/CSV mode.');
  if (hasCut && mode !== 'cut_setting') warnings.push('Template contains {Cut:...} placeholders outside Cut Setting mode.');
  if (!hasAny && mode && mode !== 'normal') warnings.push('Selected Text Mode has no matching placeholders in the template.');

  return warnings;
}
