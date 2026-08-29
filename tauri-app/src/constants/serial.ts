export const SERIAL_BAUD_RATES = [
  9600, 19200, 38400, 57600, 115200, 230400, 250000, 460800, 500000, 921600, 1000000,
];

export const SERIAL_BAUD_RATE_OPTIONS = SERIAL_BAUD_RATES.map((rate) => ({
  value: String(rate),
  label: String(rate),
}));
