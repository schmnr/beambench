// Exact GRBL K40 configurations: docs/machines/monport-usb-presets.md.
export const monportPresetCases = [
  { presetId: 'monport_k40_grbl_2_0', name: 'Monport K40 (GRBL 2.0 controller)', airOn: '', width: 300, height: 200, baud: 115200, maxSpeed: 6000, homing: true, constantPower: false, usbEvidence: true, origin: 'top_left' },
  { presetId: 'monport_k40_pro', name: 'Monport K40 Pro 40W (GRBL / G320)', airOn: '', width: 300, height: 200, baud: 115200, maxSpeed: 6000, homing: true, constantPower: false, usbEvidence: true, origin: 'top_left' },
] as const;
