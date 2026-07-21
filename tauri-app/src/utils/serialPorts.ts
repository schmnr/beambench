import type { PortInfo } from '../types/machine';

const LIKELY_USB_SERIAL_TERMS = [
  'usbserial',
  'usbmodem',
  'wchusbserial',
  'slab_usbtouart',
  'silicon labs',
  'cp210',
  'ch340',
  'ch341',
  'ftdi',
  'arduino',
  'grbl',
  'ttyusb',
  'ttyacm',
];

const NON_MACHINE_SERIAL_TERMS = [
  'debug-console',
  'bluetooth',
  'headphone',
  'hands-free',
  'incoming-port',
];

function portSearchText(port: PortInfo): string {
  return [
    port.port_name,
    port.description,
    port.manufacturer,
  ].join(' ').toLowerCase();
}

function compactPortDetails(port: PortInfo): string[] {
  const details = [port.description, port.manufacturer]
    .map((value) => value.trim())
    .filter(Boolean);
  return Array.from(new Set(details));
}

function macSerialSuffix(portName: string, prefix: '/dev/cu.' | '/dev/tty.'): string | null {
  return portName.startsWith(prefix) ? portName.slice(prefix.length) : null;
}

function hasCuTwin(port: PortInfo, ports: PortInfo[]): boolean {
  const ttySuffix = macSerialSuffix(port.port_name, '/dev/tty.');
  if (!ttySuffix) return false;
  return ports.some((candidate) => macSerialSuffix(candidate.port_name, '/dev/cu.') === ttySuffix);
}

export function isLikelyUsbSerialPort(port: PortInfo): boolean {
  const text = portSearchText(port);
  return LIKELY_USB_SERIAL_TERMS.some((term) => text.includes(term));
}

export function isObviousNonMachineSerialPort(port: PortInfo): boolean {
  const name = port.port_name.toLowerCase();
  const text = portSearchText(port);

  if (isLikelyUsbSerialPort(port)) {
    return false;
  }

  if (name.startsWith('/dev/tty.')) {
    return true;
  }

  return NON_MACHINE_SERIAL_TERMS.some((term) => text.includes(term));
}

export function serialPortLabel(port: PortInfo): string {
  const details = compactPortDetails(port);
  const detailText = details.length > 0 ? ` - ${details.join(' / ')}` : '';
  const hint = isLikelyUsbSerialPort(port) ? ' (USB serial)' : '';
  return `${port.port_name}${detailText}${hint}`;
}

export function visibleSerialPorts(
  ports: PortInfo[],
  showAllPorts: boolean,
  selectedPort?: string | null,
): PortInfo[] {
  if (showAllPorts) return ports;
  return ports.filter((port) => (
    port.port_name === selectedPort ||
    (!isObviousNonMachineSerialPort(port) && !hasCuTwin(port, ports))
  ));
}

export function hiddenSerialPortCount(
  ports: PortInfo[],
  selectedPort?: string | null,
): number {
  return ports.filter((port) => (
    port.port_name !== selectedPort &&
    (isObviousNonMachineSerialPort(port) || hasCuTwin(port, ports))
  )).length;
}

export function preferredSerialPortName(ports: PortInfo[]): string | null {
  return (
    ports.find((port) => isLikelyUsbSerialPort(port)) ??
    ports[0] ??
    null
  )?.port_name ?? null;
}
