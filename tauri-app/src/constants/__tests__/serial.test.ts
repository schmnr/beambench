import { describe, expect, it } from 'vitest';
import { SERIAL_BAUD_RATES, SERIAL_BAUD_RATE_OPTIONS } from '../serial';

describe('serial baud rates', () => {
  it('includes the 921600 baud rate required by VMS LX2b controllers', () => {
    expect(SERIAL_BAUD_RATES).toContain(921600);
  });

  it('includes the documented Marlin default and high-speed rates', () => {
    expect(SERIAL_BAUD_RATES).toEqual(expect.arrayContaining([250000, 500000, 1000000]));
  });

  it('keeps selector options synchronized with the supported rates', () => {
    expect(SERIAL_BAUD_RATE_OPTIONS).toEqual(
      SERIAL_BAUD_RATES.map((rate) => ({
        value: String(rate),
        label: String(rate),
      })),
    );
  });
});
