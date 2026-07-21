import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { useMachineStore } from '../../../stores/machineStore';
import {
  ControllerChoiceControls,
  controllerSelectionFromValue,
  controllerSelectionValue,
} from '../ControllerChoiceControls';

const initialMachineState = useMachineStore.getState();

afterEach(() => {
  cleanup();
  useMachineStore.setState(initialMachineState, true);
});

describe('ControllerChoiceControls', () => {
  it('maps every visible controller option to the shared selection contract', () => {
    expect(controllerSelectionFromValue('auto_detect')).toEqual({ mode: 'auto_detect' });
    expect(controllerSelectionFromValue('grbl')).toEqual({
      mode: 'known_driver',
      driver: 'grbl',
    });
    expect(controllerSelectionFromValue('fluid_nc')).toEqual({
      mode: 'known_driver',
      driver: 'fluid_nc',
    });
    expect(controllerSelectionFromValue('grbl_hal')).toEqual({
      mode: 'known_driver',
      driver: 'grbl_hal',
    });
    expect(controllerSelectionFromValue('laser_pecker')).toEqual({
      mode: 'known_driver',
      driver: 'laser_pecker',
    });
    expect(controllerSelectionFromValue('marlin')).toEqual({
      mode: 'known_driver',
      driver: 'marlin',
    });
    expect(controllerSelectionFromValue('snapmaker')).toEqual({
      mode: 'known_driver',
      driver: 'snapmaker',
    });
    expect(controllerSelectionFromValue('smoothieware')).toEqual({
      mode: 'known_driver',
      driver: 'smoothieware',
    });
    expect(controllerSelectionFromValue('ruida')).toEqual({
      mode: 'known_driver',
      driver: 'ruida',
    });
    expect(controllerSelectionFromValue('lihuiyu')).toEqual({
      mode: 'known_driver',
      driver: 'lihuiyu',
    });
    expect(controllerSelectionFromValue('generic_grbl_compatible')).toEqual({
      mode: 'generic_grbl_compatible',
    });
    expect(controllerSelectionValue({ mode: 'known_driver', driver: 'fluid_nc' })).toBe('fluid_nc');
    expect(controllerSelectionValue({ mode: 'known_driver', driver: 'laser_pecker' })).toBe(
      'laser_pecker',
    );
    expect(controllerSelectionValue({ mode: 'known_driver', driver: 'marlin' })).toBe('marlin');
    expect(controllerSelectionValue({ mode: 'known_driver', driver: 'snapmaker' })).toBe(
      'snapmaker',
    );
    expect(controllerSelectionValue({ mode: 'known_driver', driver: 'smoothieware' })).toBe(
      'smoothieware',
    );
    expect(controllerSelectionValue({ mode: 'known_driver', driver: 'ruida' })).toBe('ruida');
    expect(controllerSelectionValue({ mode: 'known_driver', driver: 'lihuiyu' })).toBe('lihuiyu');
  });

  it('shows only the explicit Lihuiyu choice for USB without a prompt', async () => {
    useMachineStore.setState({
      controllerSelection: { mode: 'known_driver', driver: 'grbl' },
      controllerConnectionChallenge: null,
      loading: false,
    });

    render(<ControllerChoiceControls transportKind="usb_packet" />);

    await waitFor(() => {
      expect(useMachineStore.getState().controllerSelection).toEqual({
        mode: 'known_driver',
        driver: 'lihuiyu',
      });
    });
    const select = screen.getByLabelText('Controller');
    expect(
      within(select)
        .getAllByRole('option')
        .map((option) => option.textContent),
    ).toEqual(['Lihuiyu M2/M3 Nano (Experimental)']);
    expect(screen.queryByTestId('controller-choice-challenge')).toBeNull();
  });

  it('shows exactly the serial controllers included in the public release', () => {
    useMachineStore.setState({
      controllerSelection: { mode: 'known_driver', driver: 'grbl' },
      controllerConnectionChallenge: null,
      loading: false,
    });

    render(<ControllerChoiceControls transportKind="serial" />);

    const select = screen.getByLabelText('Controller');
    expect(
      within(select)
        .getAllByRole('option')
        .map((option) => option.textContent),
    ).toEqual([
      'GRBL',
      'Auto-detect',
      'FluidNC (Experimental)',
      'grblHAL (Experimental)',
      'LaserPecker (Experimental)',
      'Marlin (Experimental)',
      'Snapmaker 2.0 (Experimental)',
      'Smoothieware (Experimental)',
      'Generic GRBL-compatible (Experimental)',
    ]);
  });

  it('shows only network-capable choices and resets a serial-only selection', async () => {
    useMachineStore.setState({
      controllerSelection: { mode: 'known_driver', driver: 'marlin' },
      controllerConnectionChallenge: null,
      loading: false,
    });

    render(<ControllerChoiceControls transportKind="tcp" />);

    await waitFor(() => {
      expect(useMachineStore.getState().controllerSelection).toEqual({ mode: 'auto_detect' });
    });
    const select = screen.getByLabelText('Controller');
    expect(
      within(select)
        .getAllByRole('option')
        .map((option) => option.textContent),
    ).toEqual([
      'Auto-detect',
      'FluidNC (Experimental)',
      'grblHAL (Experimental)',
      'LaserPecker (Experimental)',
      'Ruida (Experimental)',
    ]);
  });

  it('renders backend-owned mismatch decisions and sends the selected decision', () => {
    const continueControllerConnection = vi.fn().mockResolvedValue(undefined);
    useMachineStore.setState({
      controllerSelection: { mode: 'known_driver', driver: 'grbl' },
      controllerConnectionChallenge: {
        status: 'challenge',
        attempt_id: 'attempt-1',
        endpoint: { type: 'serial', port_name: '/dev/ttyUSB0', baud_rate: 115200 },
        detected_identity: {
          family: 'gcode',
          model: 'fluid_nc',
          firmware_identity: 'FluidNC',
          firmware_version: '4.0.3',
          evidence: ['Parsed a controller information version response'],
        },
        resolution: {
          outcome: 'mismatch_decision_required',
          selected: { mode: 'known_driver', driver: 'grbl' },
          detected_identity: {
            family: 'gcode',
            model: 'fluid_nc',
            firmware_identity: 'FluidNC',
            firmware_version: '4.0.3',
            evidence: ['Parsed a controller information version response'],
          },
          detected_driver: 'fluid_nc',
          can_remember_override: false,
          invalidated_override_reason: null,
          allowed_decisions: ['use_detected', 'continue_selected_experimentally', 'cancel'],
          override_update: { action: 'keep' },
        },
      },
      continueControllerConnection,
      loading: false,
    });

    render(<ControllerChoiceControls />);

    expect(screen.getByText('Detected: FluidNC 4.0.3')).toBeDefined();
    fireEvent.click(screen.getByRole('button', { name: 'Continue as Experimental' }));
    expect(continueControllerConnection).toHaveBeenCalledWith('continue_selected_experimentally');
  });

  it('lets the user change a blocked unavailable driver and retry the same attempt', () => {
    const setControllerSelection = vi.fn();
    const continueControllerConnection = vi.fn().mockResolvedValue(undefined);
    useMachineStore.setState({
      controllerSelection: { mode: 'known_driver', driver: 'grbl_hal' },
      controllerConnectionChallenge: {
        status: 'challenge',
        attempt_id: 'attempt-2',
        endpoint: { type: 'serial', port_name: '/dev/ttyUSB0', baud_rate: 115200 },
        detected_identity: null,
        resolution: {
          outcome: 'blocked',
          reason: 'unsupported_driver',
          message: 'The selected controller driver is not available in this build',
          override_update: { action: 'keep' },
        },
      },
      setControllerSelection,
      continueControllerConnection,
      loading: false,
    });

    render(<ControllerChoiceControls />);

    fireEvent.change(screen.getByLabelText('Controller'), { target: { value: 'grbl' } });
    expect(setControllerSelection).toHaveBeenCalledWith({
      mode: 'known_driver',
      driver: 'grbl',
    });
    fireEvent.click(screen.getByRole('button', { name: 'Try selected controller' }));
    expect(continueControllerConnection).toHaveBeenCalledWith();
  });
});
