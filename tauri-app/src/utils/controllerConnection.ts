import type { SessionState } from '../types/machine';

// Controller connection-form policy for DeviceSettingsDialog: transport
// constants, host placeholders, the driver-family default-port toggle, and
// endpoint validation. Extracted as named constants (no bare 23/50200
// literals) and pure, unit-tested functions.

export const SERIAL_TRANSPORT = 'serial' as const;
export const NETWORK_TRANSPORT = 'tcp' as const;
export const USB_TRANSPORT = 'usb_packet' as const;
export type ConnectionTransportKind =
  | typeof SERIAL_TRANSPORT
  | typeof NETWORK_TRANSPORT
  | typeof USB_TRANSPORT;

export const RUIDA_HOST_PLACEHOLDER = '192.168.1.100';
export const LASERPECKER_HOST_PLACEHOLDER = '192.168.253.1';
export const GCODE_HOST_PLACEHOLDER = 'fluidnc.local';

/** Ruida controllers listen on UDP 50200; G-code network controllers on Telnet 23. */
export const RUIDA_DEFAULT_PORT = 50200;
export const LASERPECKER_DEFAULT_PORT = 8888;
export const GCODE_DEFAULT_PORT = 23;

export type NetworkControllerKind = 'gcode' | 'laserpecker' | 'ruida';

export const ACTIVE_CONNECTION_STATES: SessionState[] = ['ready', 'running', 'paused', 'alarm'];

/**
 * Swap the network port between the driver-family defaults when the driver
 * selection changes, preserving any custom port the user typed.
 */
export function defaultPortForDriverSwitch(
  current: number,
  controller: NetworkControllerKind,
): number {
  const defaults = [GCODE_DEFAULT_PORT, LASERPECKER_DEFAULT_PORT, RUIDA_DEFAULT_PORT];
  if (!defaults.includes(current)) return current;
  if (controller === 'laserpecker') return LASERPECKER_DEFAULT_PORT;
  if (controller === 'ruida') return RUIDA_DEFAULT_PORT;
  return GCODE_DEFAULT_PORT;
}

/** Whether the connection form has a usable endpoint for the chosen transport. */
export function connectionEndpointMissing(
  transportKind: ConnectionTransportKind,
  networkHost: string,
  networkPort: number,
  selectedUsbDeviceId: string,
  selectedPort: string,
): boolean {
  if (transportKind === NETWORK_TRANSPORT) {
    return (
      networkHost.trim() === '' ||
      !Number.isInteger(networkPort) ||
      networkPort < 1 ||
      networkPort > 65535
    );
  }
  if (transportKind === USB_TRANSPORT) {
    return selectedUsbDeviceId === '';
  }
  return selectedPort === '';
}
