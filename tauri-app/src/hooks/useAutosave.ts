import { useEffect, useRef } from 'react';
import { useAppStore } from '../stores/appStore';
import { useProjectStore } from '../stores/projectStore';
import { persistenceService } from '../services/persistenceService';

/**
 * Autosave hook: periodically saves a recovery copy of the project
 * when autosave is enabled and the project is dirty.
 */
export function useAutosave() {
  const settings = useAppStore((s) => s.settings);
  const project = useProjectStore((s) => s.project);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const inFlightRef = useRef(false);

  const enabled = settings?.autosave_enabled ?? true;
  const intervalSecs = settings?.autosave_interval_secs ?? 120;
  const isDirty = project?.dirty === true;

  useEffect(() => {
    if (timerRef.current) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
    inFlightRef.current = false;

    if (!enabled || !isDirty) return;

    let cancelled = false;

    const scheduleNext = () => {
      if (cancelled) return;
      timerRef.current = setTimeout(() => {
        void runAutosave();
      }, intervalSecs * 1000);
    };

    const runAutosave = async () => {
      if (cancelled || inFlightRef.current) {
        scheduleNext();
        return;
      }

      inFlightRef.current = true;
      try {
        await persistenceService.autosave();
      } catch {
        // Autosave failures are silent — they shouldn't interrupt the user
      } finally {
        inFlightRef.current = false;
        scheduleNext();
      }
    };

    scheduleNext();

    return () => {
      cancelled = true;
      inFlightRef.current = false;
      if (timerRef.current) {
        clearTimeout(timerRef.current);
        timerRef.current = null;
      }
    };
  }, [enabled, intervalSecs, isDirty]);
}
