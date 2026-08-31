import { describe, expect, it } from 'vitest';
import {
  GCODE_DEFAULT_PORT,
  LASERPECKER_DEFAULT_PORT,
  RUIDA_DEFAULT_PORT,
  XTOOL_M1_DEFAULT_PORT,
  XTOOL_M1_MAC_HOST,
  XTOOL_M1_WINDOWS_LINUX_HOST,
  connectionEndpointMissing,
  defaultPortForDriverSwitch,
  xtoolM1DefaultHost,
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

  it('switches to the original xTool M1 HTTP default', () => {
    expect(defaultPortForDriverSwitch(GCODE_DEFAULT_PORT, 'xtool_m1')).toBe(XTOOL_M1_DEFAULT_PORT);
    expect(defaultPortForDriverSwitch(RUIDA_DEFAULT_PORT, 'xtool_m1')).toBe(XTOOL_M1_DEFAULT_PORT);
  });

  it('preserves a user-entered custom port across a driver switch', () => {
    expect(defaultPortForDriverSwitch(9000, 'ruida')).toBe(9000);
    expect(defaultPortForDriverSwitch(9000, 'laserpecker')).toBe(9000);
    expect(defaultPortForDriverSwitch(9000, 'xtool_m1')).toBe(9000);
    expect(defaultPortForDriverSwitch(9000, 'gcode')).toBe(9000);
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

describe('xtoolM1DefaultHost', () => {
  it('uses the M1 USB-network address for the current platform family', () => {
    expect(xtoolM1DefaultHost({ platform: 'MacIntel', userAgent: '' })).toBe(XTOOL_M1_MAC_HOST);
    expect(xtoolM1DefaultHost({ platform: 'Win32', userAgent: '' })).toBe(
      XTOOL_M1_WINDOWS_LINUX_HOST,
    );
    expect(xtoolM1DefaultHost({ platform: 'Linux x86_64', userAgent: '' })).toBe(
      XTOOL_M1_WINDOWS_LINUX_HOST,
    );
  });
});
