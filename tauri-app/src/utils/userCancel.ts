/**
 * True only for the exact messages a cancelled dialog or superseded preview
 * produces ("Save cancelled", "Export cancelled", "Plan generation
 * cancelled"). Matching any text that merely contains "cancelled" hid real
 * failures, such as a disk error that says "device cancelled the I/O request".
 */
export function isUserCancel(error: unknown): boolean {
  const text = (error instanceof Error ? error.message : String(error))
    .replace(/^(?:Error:\s*)+/u, '')
    .trim();
  return /^(?:Save|Open|Export|Import|Plan generation) cancelled\.?$/u.test(text);
}
