import { invoke as tauriInvoke, type InvokeArgs, type InvokeOptions } from '@tauri-apps/api/core';
import { useRemoteHostStore } from '../stores/remoteHostStore';
import type { FeedbackSourceContext } from '../types/feedback';

const localMachineCommands = new Set([
  'begin_controller_connection',
  'begin_network_controller_connection',
  'begin_usb_controller_connection',
  'cancel_job',
  'connect_machine',
  'connect_machine_candidate',
  'continue_controller_connection',
  'disconnect_machine',
  'emergency_stop',
  'frame_job',
  'laser_fire_keepalive',
  'laser_fire_start',
  'laser_fire_stop',
  'machine_home',
  'machine_jog',
  'machine_jog_cancel',
  'machine_unlock',
  'move_laser_to',
  'move_laser_to_machine',
  'move_laser_to_project_point',
  'pause_job',
  'reset_all_overrides',
  'reset_work_origin',
  'resume_job',
  'run_macro',
  'run_preflight_check',
  'send_gcode_line',
  'set_feed_override',
  'set_grbl_setting',
  'set_spindle_override',
  'set_work_origin',
  'start_job',
  'quality_test_frame',
  'quality_test_start',
]);

const failures: { command: string; message: string; at: number }[] = [];

/** Preserve the failed command name without retaining command arguments. */
export async function invoke<T>(
  command: string,
  ...args: [InvokeArgs?, InvokeOptions?]
): Promise<T> {
  try {
    if (useRemoteHostStore.getState().enabled && command === 'emergency_stop') {
      await tauriInvoke('remote_host_request', {
        path: '/api/v1/machine/emergency-stop',
        body: {},
      });
      return undefined as T;
    }
    if (useRemoteHostStore.getState().enabled && localMachineCommands.has(command)) {
      throw new Error(
        'Use the remote host controls in the Laser panel while remote mode is selected.',
      );
    }
    return await tauriInvoke<T>(command, ...args);
  } catch (error) {
    failures.push({ command, message: String(error), at: Date.now() });
    if (failures.length > 32) failures.shift();
    throw error;
  }
}

export function invocationFailureContext(
  message: string,
  localize: (detail: string) => string,
): Partial<FeedbackSourceContext> | undefined {
  const failure = [...failures].reverse().find((entry) => {
    const age = Date.now() - entry.at;
    return (
      age >= 0 &&
      age <= 30_000 &&
      (message === entry.message || message === localize(entry.message))
    );
  });
  if (!failure) return undefined;
  return {
    action: failure.command,
    correlation_ts: new Date(failure.at).toISOString(),
    error_details: { backend_error: failure.message },
  };
}
