import { useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useMachineStore } from '../../stores/machineStore';
import { usePreviewStore } from '../../stores/previewStore';

interface JobControlPanelProps {
  onShowPreflight: () => void;
}

export function JobControlPanel({ onShowPreflight }: JobControlPanelProps) {
  const { t } = useTranslation();
  const startInFlightRef = useRef(false);
  const [startInFlight, setStartInFlight] = useState(false);
  const sessionState = useMachineStore((s) => s.sessionState);
  const machineStatus = useMachineStore((s) => s.machineStatus);
  const jobProgress = useMachineStore((s) => s.jobProgress);
  const loading = useMachineStore((s) => s.loading);
  const runPreflight = useMachineStore((s) => s.runPreflight);
  const startJob = useMachineStore((s) => s.startJob);
  const pauseJob = useMachineStore((s) => s.pauseJob);
  const resumeJob = useMachineStore((s) => s.resumeJob);
  const cancelJob = useMachineStore((s) => s.cancelJob);
  const previewState = usePreviewStore((s) => s.state);
  const generatePreview = usePreviewStore((s) => s.generatePreview);

  const canStart =
    sessionState === 'ready' &&
    machineStatus?.run_state === 'idle' &&
    !loading &&
    previewState !== 'generating' &&
    !startInFlight;

  const canPause = jobProgress?.state === 'running';
  const canResume = jobProgress?.state === 'paused';
  const canCancel =
    jobProgress?.state === 'preparing' || jobProgress?.state === 'running' || jobProgress?.state === 'paused';

  const handleStart = async () => {
    if (startInFlightRef.current) return;

    startInFlightRef.current = true;
    setStartInFlight(true);

    try {
      let previewReady = previewState === 'current';
      if (!previewReady) {
        previewReady = await generatePreview();
      }
      if (!previewReady) {
        return;
      }

      const report = await runPreflight();
      if (!report) return;
      if (report.outcome === 'pass') {
        await startJob();
      } else {
        onShowPreflight();
      }
    } finally {
      startInFlightRef.current = false;
      setStartInFlight(false);
    }
  };

  return (
    <div className="flex flex-row gap-2 flex-wrap">
      <button
        className="text-xs px-3 py-1.5 rounded bg-bb-success text-bb-on-success hover:bg-bb-success-hover disabled:opacity-60 disabled:cursor-not-allowed"
        disabled={!canStart}
        onClick={handleStart}
      >
        {t('panels.machine.job_control.start')}
      </button>
      <button
        className="text-xs px-3 py-1.5 rounded bg-bb-warning text-bb-on-warning hover:bg-bb-warning-hover disabled:opacity-60 disabled:cursor-not-allowed"
        disabled={!canPause}
        onClick={pauseJob}
      >
        {t('panels.machine.job_control.pause')}
      </button>
      <button
        className="text-xs px-3 py-1.5 rounded bg-bb-accent text-bb-on-accent hover:bg-bb-accent-hover disabled:opacity-60 disabled:cursor-not-allowed"
        disabled={!canResume}
        onClick={resumeJob}
      >
        {t('panels.machine.job_control.resume')}
      </button>
      <button
        className="text-xs px-3 py-1.5 rounded bg-bb-error text-bb-on-error hover:bg-bb-error-hover disabled:opacity-60 disabled:cursor-not-allowed"
        disabled={!canCancel}
        onClick={cancelJob}
      >
        {t('panels.machine.job_control.cancel')}
      </button>
    </div>
  );
}
