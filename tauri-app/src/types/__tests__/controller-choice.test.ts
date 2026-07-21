import { describe, expect, it } from 'vitest';
import type {
  ControllerChoiceOutcome,
  ControllerChoiceResolution,
  ControllerSelection,
} from '../machine';

function outcomeName(outcome: ControllerChoiceOutcome): string {
  switch (outcome.outcome) {
    case 'resolved':
    case 'selection_required':
    case 'mismatch_decision_required':
    case 'cancelled':
    case 'blocked':
      return outcome.outcome;
    default: {
      const exhaustive: never = outcome;
      return exhaustive;
    }
  }
}

describe('controller choice wire contract', () => {
  it('uses the same tagged selection shapes as Rust serde', () => {
    const selections: ControllerSelection[] = [
      { mode: 'auto_detect' },
      { mode: 'known_driver', driver: 'grbl' },
      { mode: 'known_driver', driver: 'fluid_nc' },
      { mode: 'known_driver', driver: 'grbl_hal' },
      { mode: 'known_driver', driver: 'marlin' },
      { mode: 'known_driver', driver: 'snapmaker' },
      { mode: 'known_driver', driver: 'smoothieware' },
      { mode: 'generic_grbl_compatible' },
      { mode: 'unknown' },
    ];

    expect(selections.map((selection) => selection.mode)).toEqual([
      'auto_detect',
      'known_driver',
      'known_driver',
      'known_driver',
      'known_driver',
      'known_driver',
      'known_driver',
      'generic_grbl_compatible',
      'unknown',
    ]);
  });

  it('keeps policy resolution separate from connected or Ready state', () => {
    const resolution: ControllerChoiceResolution = {
      outcome: 'resolved',
      choice: {
        selection: { mode: 'known_driver', driver: 'grbl' },
        driver: 'grbl',
        source: 'known_driver_selection',
        detected_identity: null,
        requires_experimental_mode: false,
        mismatch: false,
        override_scope: null,
        requires_experimental_compatibility_handshake: false,
      },
      override_update: { action: 'keep' },
    };

    expect(outcomeName(resolution)).toBe('resolved');
    expect('session_state' in resolution).toBe(false);
    expect('capabilities' in resolution).toBe(false);
  });

  it('carries backend-owned decision allowlists and override updates', () => {
    const resolution: ControllerChoiceResolution = {
      outcome: 'mismatch_decision_required',
      selected: { mode: 'generic_grbl_compatible' },
      detected_identity: {
        family: 'gcode',
        model: 'grbl',
        firmware_identity: 'Grbl',
        firmware_version: '1.1h',
        evidence: ['Parsed startup banner'],
      },
      detected_driver: 'grbl',
      can_remember_override: true,
      invalidated_override_reason: 'firmware_identity_changed',
      allowed_decisions: [
        'use_detected',
        'continue_selected_experimentally',
        'cancel',
      ],
      override_update: {
        action: 'clear',
        reason: 'firmware_identity_changed',
      },
    };

    expect(outcomeName(resolution)).toBe('mismatch_decision_required');
    expect(resolution.allowed_decisions).toContain('cancel');
    expect(resolution.override_update.action).toBe('clear');
  });
});
