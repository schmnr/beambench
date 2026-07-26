export const ROTARY_SETUP_OPEN_EVENT = 'beam-bench:rotary-setup-open';

export function openRotarySetup(): void {
  window.dispatchEvent(new Event(ROTARY_SETUP_OPEN_EVENT));
}
