import { useEffect, useRef } from 'react';
import { useMachineStore } from '../stores/machineStore';
import { useVariableTextStore } from '../stores/variableTextStore';

/**
 * Polling hook for machine status and job progress.
 * - Status poll (adaptive): 2000ms when idle (ready), 500ms when active (running/paused/alarm)
 * - Job poll (250ms): active when job is running or paused
 */
export function useMachinePolling() {
  const sessionState = useMachineStore((s) => s.sessionState);
  const jobProgress = useMachineStore((s) => s.jobProgress);
  const refreshStatus = useMachineStore((s) => s.refreshStatus);
  const refreshSessionState = useMachineStore((s) => s.refreshSessionState);
  const refreshJobProgress = useMachineStore((s) => s.refreshJobProgress);
  const hydrateSession = useMachineStore((s) => s.hydrateSession);

  // Backend session discovery: if the CLI/API connects while the frontend
  // thinks it is disconnected, keep probing cheaply until the store catches up.
  useEffect(() => {
    if (sessionState !== 'disconnected') return;

    hydrateSession();

    const interval = setInterval(() => {
      hydrateSession();
    }, 2000);

    return () => clearInterval(interval);
  }, [sessionState, hydrateSession]);

  // Status polling: active when connected, adaptive interval
  useEffect(() => {
    const connectedStates = ['ready', 'running', 'paused', 'alarm'];
    if (!connectedStates.includes(sessionState)) return;

    const activeStates = ['running', 'paused', 'alarm'];
    const interval_ms = activeStates.includes(sessionState) ? 500 : 2000;

    refreshStatus();
    refreshSessionState();

    const interval = setInterval(() => {
      refreshStatus();
      refreshSessionState();
    }, interval_ms);

    return () => clearInterval(interval);
  }, [sessionState, refreshStatus, refreshSessionState]);

  // Job polling: active when job is running or paused
  // Auto-clear terminal job states after a brief display period
  const prevJobStateRef = useRef<string | null>(null);
  useEffect(() => {
    const currentState = jobProgress?.state ?? null;
    const prevState = prevJobStateRef.current;
    prevJobStateRef.current = currentState;

    if (!jobProgress) return;

    // Capture job start time when transitioning to running
    if (currentState === 'running' && prevState !== 'running') {
      useVariableTextStore.getState().captureJobStartTime();
    }

    // Preparing is a potentially long staged transfer (Ruida upload, Lihuiyu
    // packet stream); poll it so progress and terminal transitions don't rely
    // solely on event delivery.
    const activeJobStates = ['preparing', 'running', 'paused'];
    if (activeJobStates.includes(jobProgress.state)) {
      const interval = setInterval(() => {
        refreshJobProgress();
      }, 250);
      return () => clearInterval(interval);
    }

    // Terminal states (completed, failed, cancelled): clear after 3s so user sees final status
    const terminalStates = ['completed', 'failed', 'cancelled'];
    if (terminalStates.includes(jobProgress.state)) {
      const timeout = setTimeout(() => {
        useMachineStore.setState({ jobProgress: null });
        useVariableTextStore.getState().clearJobStartTime();
      }, 3000);
      return () => clearTimeout(timeout);
    }
  }, [jobProgress, refreshJobProgress]);
}
