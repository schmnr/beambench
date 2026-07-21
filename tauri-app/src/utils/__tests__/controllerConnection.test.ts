import { describe, expect, it } from 'vitest';
import {
  GCODE_DEFAULT_PORT,
  LASERPECKER_DEFAULT_PORT,
  RUIDA_DEFAULT_PORT,
  connectionEndpointMissing,
  defaultPortForDriverSwitch,
} from '../controllerConnection';

describe('defaultPortForDriverSwitch', () => {
  it('switches to the Ruida UDP port only from the default G-code port', () => {
    expect(defaultPortForDriverSwitch(GCODE_DEFAULT_PORT, 'ruida')).toBe(RUIDA_DEFAULT_PORT);
  });

  it('switches back to the G-code port only from the default Ruida port', () => {
    expect(defaultPortForDriverSwitch(RUIDA_DEFAULT_PORT, 'gcode')).toBe(GCODE_DEFAULT_PORT);
  });

  it('switches to the LaserPecker LX2 TCP default', () => {
    expect(defaultPortForDriverSwitch(GCODE_DEFAULT_PORT, 'laserpecker')).toBe(
      LASERPECKER_DEFAULT_PORT,
    );
    expect(defaultPortForDriverSwitch(RUIDA_DEFAULT_PORT, 'laserpecker')).toBe(
      LASERPECKER_DEFAULT_PORT,
    );
  });

  it('preserves a user-entered custom port across a driver switch', () => {
    expect(defaultPortForDriverSwitch(8080, 'ruida')).toBe(8080);
    expect(defaultPortForDriverSwitch(8080, 'laserpecker')).toBe(8080);
    expect(defaultPortForDriverSwitch(8080, 'gcode')).toBe(8080);
  });
});

describe('connectionEndpointMissing', () => {
  it('requires a host and valid port for network transport', () => {
    expect(connectionEndpointMissing('tcp', '', 50200, '', '')).toBe(true);
    expect(connectionEndpointMissing('tcp', '10.0.0.5', 0, '', '')).toBe(true);
    expect(connectionEndpointMissing('tcp', '10.0.0.5', 70000, '', '')).toBe(true);
    expect(connectionEndpointMissing('tcp', '10.0.0.5', 50200, '', '')).toBe(false);
  });

  it('requires a selected device for USB transport', () => {
    expect(connectionEndpointMissing('usb_packet', '', 0, '', 'ignored')).toBe(true);
    expect(connectionEndpointMissing('usb_packet', '', 0, 'usb-1', '')).toBe(false);
  });

  it('requires a selected port for serial transport', () => {
    expect(connectionEndpointMissing('serial', '', 0, '', '')).toBe(true);
    expect(connectionEndpointMissing('serial', '', 0, '', '/dev/ttyUSB0')).toBe(false);
  });
});
