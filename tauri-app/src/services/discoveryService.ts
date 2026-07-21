import { invoke } from '@tauri-apps/api/core';
import type {
  DiscoveryScanState,
  DiscoveryTcpTarget,
  DiscoveryUsbTarget,
  MachineProfile,
  SessionState,
} from '../types/machine';

export const discoveryService = {
  async startDiscovery(
    tcpTargets: DiscoveryTcpTarget[] = [],
    usbTargets: DiscoveryUsbTarget[] = [],
  ): Promise<DiscoveryScanState> {
    return invoke<DiscoveryScanState>('start_machine_discovery', {
      tcpTargets,
      usbTargets,
    });
  },

  async getDiscoveryState(): Promise<DiscoveryScanState> {
    return invoke<DiscoveryScanState>('get_machine_discovery_state');
  },

  async cancelDiscovery(): Promise<DiscoveryScanState> {
    return invoke<DiscoveryScanState>('cancel_machine_discovery');
  },

  async connectCandidate(candidateId: string): Promise<SessionState> {
    return invoke<SessionState>('connect_machine_candidate', { candidateId });
  },

  async bootstrapProfile(
    candidateId: string,
    profileName?: string,
    activate = true,
  ): Promise<MachineProfile> {
    return invoke<MachineProfile>('bootstrap_machine_profile', {
      candidateId,
      profileName,
      activate,
    });
  },
};
