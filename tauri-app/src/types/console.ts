/** Console types for the G-code console panel. */

export type ConsoleDirection = 'sent' | 'received';

// backend `ConsoleEntry` is `{ timestamp, direction, content }` —
// no `is_error` field. UI-side error highlighting derives from the payload
// (e.g. messages starting with "error:"), so the frontend mirror must not
// invent an optional field the backend never emits.
export interface ConsoleEntry {
  timestamp: string;
  direction: ConsoleDirection;
  content: string;
}
