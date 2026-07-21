import { useTranslation } from 'react-i18next';
import { useProjectStore } from '../../stores/projectStore';
import type { TransformLocks } from '../../types/project';

const toggleKeys: { key: keyof TransformLocks; labelKey: string }[] = [
  { key: 'move_enabled', labelKey: 'toolbars.transform_toggles.move' },
  { key: 'size_enabled', labelKey: 'toolbars.transform_toggles.size' },
  { key: 'rotate_enabled', labelKey: 'toolbars.transform_toggles.rotate' },
  { key: 'shear_enabled', labelKey: 'toolbars.transform_toggles.shear' },
];

export function TransformToggles() {
  const { t } = useTranslation();
  const project = useProjectStore((s) => s.project);
  const setTransformLocks = useProjectStore((s) => s.setTransformLocks);

  // TransformLocks is non-optional; default mirrors backend `Default` (all enabled).
  const locks: TransformLocks = project?.transform_locks ?? {
    move_enabled: true,
    size_enabled: true,
    rotate_enabled: true,
    shear_enabled: true,
  };

  const handleToggle = (key: keyof TransformLocks) => {
    void setTransformLocks({ ...locks, [key]: !locks[key] });
  };

  const isLocked = (key: keyof TransformLocks) => locks[key] === false;

  return (
    <div className="no-select flex items-center h-6 bg-bb-panel px-3 gap-2 text-xs border-t border-bb-border">
      {toggleKeys.map(({ key, labelKey }) => (
        <button
          key={key}
          onClick={() => handleToggle(key)}
          className={`px-1.5 py-0.5 rounded text-xs ${
            isLocked(key)
              ? 'bg-bb-accent/15 border border-bb-accent/30 text-bb-text'
              : 'text-bb-text-muted hover:text-bb-text hover:bg-bb-surface'
          }`}
        >
          {t(labelKey)}
        </button>
      ))}
    </div>
  );
}
