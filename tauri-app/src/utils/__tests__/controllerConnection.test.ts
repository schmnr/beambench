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
  normalizeNetworkEndpoint,
  xtoolM1DefaultHost,
} from '../controllerConnection';

describe('defaultPortForDriverSwitch', () => {
  it('switches to the Ruida UDP port only from the default G-code port', () => {
    expect(defaultPortForDriverSwitch(GCODE_DEFAULT_PORT, 'gcode', 'ruida')).toBe(
      RUIDA_DEFAULT_PORT,
    );
  });

  it('switches back to the G-code port only from the default Ruida port', () => {
    expect(defaultPortForDriverSwitch(RUIDA_DEFAULT_PORT, 'ruida', 'gcode')).toBe(
      GCODE_DEFAULT_PORT,
    );
  });

  it('switches to the LaserPecker LX2 TCP default', () => {
    expect(defaultPortForDriverSwitch(GCODE_DEFAULT_PORT, 'gcode', 'laserpecker')).toBe(
      LASERPECKER_DEFAULT_PORT,
    );
    expect(defaultPortForDriverSwitch(RUIDA_DEFAULT_PORT, 'ruida', 'laserpecker')).toBe(
      LASERPECKER_DEFAULT_PORT,
    );
  });

  it('switches to the original xTool M1 HTTP default', () => {
    expect(defaultPortForDriverSwitch(GCODE_DEFAULT_PORT, 'gcode', 'xtool_m1')).toBe(
      XTOOL_M1_DEFAULT_PORT,
    );
    expect(defaultPortForDriverSwitch(RUIDA_DEFAULT_PORT, 'ruida', 'xtool_m1')).toBe(
      XTOOL_M1_DEFAULT_PORT,
    );
  });

  it('preserves a user-entered custom port across a driver switch', () => {
    expect(defaultPortForDriverSwitch(9000, 'gcode', 'ruida')).toBe(9000);
    expect(defaultPortForDriverSwitch(9000, 'ruida', 'laserpecker')).toBe(9000);
    expect(defaultPortForDriverSwitch(9000, 'laserpecker', 'xtool_m1')).toBe(9000);
    expect(defaultPortForDriverSwitch(9000, 'xtool_m1', 'gcode')).toBe(9000);
  });

  it('does not reinterpret another driver default as the previous driver default', () => {
    expect(defaultPortForDriverSwitch(XTOOL_M1_DEFAULT_PORT, 'gcode', 'ruida')).toBe(
      XTOOL_M1_DEFAULT_PORT,
    );
  });
});

describe('normalizeNetworkEndpoint', () => {
  it('moves a pasted IPv4 or hostname port into the port field', () => {
    expect(normalizeNetworkEndpoint('10.0.1.155:8080', 23)).toEqual({
      host: '10.0.1.155',
      port: 8080,
    });
    expect(normalizeNetworkEndpoint('laser.local:50200', 23)).toEqual({
      host: 'laser.local',
      port: 50200,
    });
  });

  it('supports bracketed IPv6 and preserves bare IPv6', () => {
    expect(normalizeNetworkEndpoint('[fe80::1]:8080', 23)).toEqual({
      host: 'fe80::1',
      port: 8080,
    });
    expect(normalizeNetworkEndpoint('fe80::1', 23)).toEqual({ host: 'fe80::1', port: 23 });
  });

  it('lets endpoint validation reject an out-of-range pasted port', () => {
    const endpoint = normalizeNetworkEndpoint('10.0.1.155:70000', 23);
    expect(connectionEndpointMissing('tcp', endpoint.host, endpoint.port, '', '')).toBe(true);
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
