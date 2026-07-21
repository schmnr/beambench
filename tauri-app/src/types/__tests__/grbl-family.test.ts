import { describe, expect, it } from 'vitest';
import type {
  ControllerModel,
  GrblFamilyDialect,
  GrblFamilyIdentity,
  GrblFamilyIdentityEvidence,
  GrblFamilyIdentityStatus,
} from '../machine';

describe('GRBL-family identity wire contract', () => {
  it('keeps Marlin-derived models distinct from the GRBL family', () => {
    const models: ControllerModel[] = ['marlin', 'snapmaker', 'smoothieware'];
    const dialects: GrblFamilyDialect[] = ['unknown', 'grbl', 'fluid_nc', 'grbl_hal'];

    expect(models).toEqual(['marlin', 'snapmaker', 'smoothieware']);
    expect(models.every((model) => !dialects.includes(model as GrblFamilyDialect))).toBe(true);
  });

  it('mirrors the Rust dialect, status, and evidence strings', () => {
    const dialects: GrblFamilyDialect[] = ['unknown', 'grbl', 'fluid_nc', 'grbl_hal'];
    const models: ControllerModel[] = ['unknown', 'grbl', 'fluid_nc', 'grbl_hal'];
    const statuses: GrblFamilyIdentityStatus[] = [
      'unknown',
      'protocol_compatible',
      'provisional',
      'identified',
      'conflicting',
    ];
    const evidence: GrblFamilyIdentityEvidence[] = [
      'startup_banner',
      'protocol_signature',
      'controller_info_version',
      'firmware_identity_message',
    ];

    expect(dialects).toEqual(models);
    expect(statuses).toContain('identified');
    expect(evidence).toEqual([
      'startup_banner',
      'protocol_signature',
      'controller_info_version',
      'firmware_identity_message',
    ]);
  });

  it('uses a payload-free evidence list in the serialized identity shape', () => {
    const identity: GrblFamilyIdentity = {
      dialect: 'fluid_nc',
      status: 'identified',
      firmware_identity: 'FluidNC',
      firmware_version: '3.9.1',
      evidence: ['controller_info_version'],
    };

    expect(identity).toEqual({
      dialect: 'fluid_nc',
      status: 'identified',
      firmware_identity: 'FluidNC',
      firmware_version: '3.9.1',
      evidence: ['controller_info_version'],
    });
    expect(typeof identity.evidence[0]).toBe('string');
  });
});
