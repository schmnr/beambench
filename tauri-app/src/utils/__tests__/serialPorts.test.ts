import { describe, expect, it } from 'vitest';
import type { PortInfo } from '../../types/machine';
import {
  hiddenSerialPortCount,
  isLikelyUsbSerialPort,
  isObviousNonMachineSerialPort,
  preferredSerialPortName,
  serialPortLabel,
  visibleSerialPorts,
} from '../serialPorts';

function port(port_name: string, overrides: Partial<PortInfo> = {}): PortInfo {
  return {
    port_name,
    description: '',
    manufacturer: '',
    vid: null,
    pid: null,
    ...overrides,
  };
}

describe('serialPorts', () => {
  it('hides macOS system and Bluetooth ports by default', () => {
    const ports = [
      port('/dev/cu.debug-console'),
      port('/dev/tty.debug-console'),
      port('/dev/cu.NatesHeadphones'),
      port('/dev/cu.Bluetooth-Incoming-Port'),
      port('/dev/cu.usbserial-1420'),
    ];

    expect(visibleSerialPorts(ports, false).map((item) => item.port_name)).toEqual([
      '/dev/cu.usbserial-1420',
    ]);
    expect(hiddenSerialPortCount(ports)).toBe(4);
  });

  it('keeps Linux ttyUSB and ttyACM ports visible', () => {
    const ports = [
      port('/dev/ttyUSB0'),
      port('/dev/ttyACM0'),
      port('/dev/tty.debug-console'),
    ];

    expect(visibleSerialPorts(ports, false).map((item) => item.port_name)).toEqual([
      '/dev/ttyUSB0',
      '/dev/ttyACM0',
    ]);
  });

  it('keeps a macOS tty USB serial port visible when no cu twin exists', () => {
    const ports = [
      port('/dev/tty.usbserial-210'),
      port('/dev/tty.debug-console'),
    ];

    expect(visibleSerialPorts(ports, false).map((item) => item.port_name)).toEqual([
      '/dev/tty.usbserial-210',
    ]);
    expect(hiddenSerialPortCount(ports)).toBe(1);
  });

  it('prefers the macOS cu USB serial port when a tty twin exists', () => {
    const ports = [
      port('/dev/cu.usbserial-210'),
      port('/dev/tty.usbserial-210'),
      port('/dev/cu.Bluetooth-Incoming-Port'),
    ];

    expect(visibleSerialPorts(ports, false).map((item) => item.port_name)).toEqual([
      '/dev/cu.usbserial-210',
    ]);
    expect(hiddenSerialPortCount(ports)).toBe(2);
  });

  it('keeps a selected hidden port visible until the user changes it', () => {
    const ports = [
      port('/dev/cu.debug-console'),
      port('/dev/cu.usbserial-1420'),
    ];

    expect(visibleSerialPorts(ports, false, '/dev/cu.debug-console').map((item) => item.port_name)).toEqual([
      '/dev/cu.debug-console',
      '/dev/cu.usbserial-1420',
    ]);
    expect(hiddenSerialPortCount(ports, '/dev/cu.debug-console')).toBe(0);
  });

  it('labels likely USB serial adapters with useful device details', () => {
    const adapter = port('/dev/cu.SLAB_USBtoUART', {
      description: 'CP2102 USB to UART Bridge Controller',
      manufacturer: 'Silicon Labs',
    });

    expect(isLikelyUsbSerialPort(adapter)).toBe(true);
    expect(isObviousNonMachineSerialPort(adapter)).toBe(false);
    expect(serialPortLabel(adapter)).toBe(
      '/dev/cu.SLAB_USBtoUART - CP2102 USB to UART Bridge Controller / Silicon Labs (USB serial)',
    );
  });

  it('chooses a likely USB serial port before a generic visible port', () => {
    const ports = [
      port('/dev/cu.SomeOtherSerial'),
      port('/dev/cu.usbserial-210'),
    ];

    expect(preferredSerialPortName(ports)).toBe('/dev/cu.usbserial-210');
  });
});
