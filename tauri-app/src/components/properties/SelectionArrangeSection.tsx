import { useTranslation } from 'react-i18next';
import {
  AlignCenterHorizontal,
  AlignCenterVertical,
  AlignEndHorizontal,
  AlignEndVertical,
  AlignHorizontalSpaceAround,
  AlignStartHorizontal,
  AlignStartVertical,
  AlignVerticalSpaceAround,
  Group,
  Ungroup,
} from 'lucide-react';
import type { AlignmentType, DistributionDirection } from '../../types/project';
import { useProjectStore } from '../../stores/projectStore';
import { IconButton } from '../shared/IconButton';
import { INSPECTOR_SECTION_HEADER_CLASS } from '../shared/panelAppearance';

const COMPACT_BUTTON_SIZE = 'xs' as const;

const ALIGNMENTS: Array<{
  type: AlignmentType;
  labelKey: string;
  icon: React.ReactNode;
}> = [
  { type: 'left', labelKey: 'toolbars.main.align_left', icon: <AlignStartVertical size={17} /> },
  { type: 'right', labelKey: 'toolbars.main.align_right', icon: <AlignEndVertical size={17} /> },
  { type: 'top', labelKey: 'toolbars.main.align_top', icon: <AlignStartHorizontal size={17} /> },
  { type: 'bottom', labelKey: 'toolbars.main.align_bottom', icon: <AlignEndHorizontal size={17} /> },
  { type: 'centers_v', labelKey: 'toolbars.main.align_vertical_centers', icon: <AlignCenterHorizontal size={17} /> },
  { type: 'centers_h', labelKey: 'toolbars.main.align_horizontal_centers', icon: <AlignCenterVertical size={17} /> },
];

const DISTRIBUTIONS: Array<{
  direction: DistributionDirection;
  labelKey: string;
  icon: React.ReactNode;
}> = [
  {
    direction: 'h_centered',
    labelKey: 'toolbars.main.distribute_h_centered',
    icon: <AlignHorizontalSpaceAround size={17} />,
  },
  {
    direction: 'v_centered',
    labelKey: 'toolbars.main.distribute_v_centered',
    icon: <AlignVerticalSpaceAround size={17} />,
  },
];

/** Contextual relationship controls for the current object selection. */
export function SelectionArrangeSection() {
  const { t } = useTranslation();
  const project = useProjectStore((s) => s.project);
  const selectedObjectIds = useProjectStore((s) => s.selectedObjectIds);
  const groupObjects = useProjectStore((s) => s.groupObjects);
  const ungroupObjects = useProjectStore((s) => s.ungroupObjects);
  const alignObjects = useProjectStore((s) => s.alignObjects);
  const distributeObjects = useProjectStore((s) => s.distributeObjects);

  const selectedObjects = project?.objects.filter((object) => selectedObjectIds.includes(object.id)) ?? [];
  const anyLocked = selectedObjects.some((object) => object.locked);
  const isMultiSelection = selectedObjectIds.length >= 2;
  const isSingleGroup =
    selectedObjectIds.length === 1 && selectedObjects[0]?.data.type === 'group';

  if (!isMultiSelection && !isSingleGroup) return null;

  const canGroup = isMultiSelection && !anyLocked;
  const canUngroup = isSingleGroup && !anyLocked;
  const canAlign = isMultiSelection && !anyLocked;
  const canDistribute = selectedObjectIds.length >= 3 && !anyLocked;

  return (
    <section className="border-t border-bb-border pt-3" data-testid="selection-arrange-section">
      <div className={INSPECTOR_SECTION_HEADER_CLASS}>{t('menus.arrange.label')}</div>

      <div className="mt-1 flex gap-0.5">
        <IconButton
          size={COMPACT_BUTTON_SIZE}
          icon={<Group size={17} />}
          label={t('toolbars.main.group')}
          disabled={!canGroup}
          onClick={() => void groupObjects(selectedObjectIds)}
        />
        <IconButton
          size={COMPACT_BUTTON_SIZE}
          icon={<Ungroup size={17} />}
          label={t('toolbars.main.ungroup')}
          disabled={!canUngroup}
          onClick={() => void ungroupObjects(selectedObjectIds[0])}
        />
      </div>

      {isMultiSelection && (
        <>
          <div className={`${INSPECTOR_SECTION_HEADER_CLASS} mt-3`}>{t('menus.arrange.align')}</div>
          <div className="mt-1 grid grid-cols-6 gap-0.5">
            {ALIGNMENTS.map(({ type, labelKey, icon }) => (
              <IconButton
                key={type}
                size={COMPACT_BUTTON_SIZE}
                icon={icon}
                label={t(labelKey)}
                disabled={!canAlign}
                onClick={() => void alignObjects(selectedObjectIds, type)}
              />
            ))}
          </div>
        </>
      )}

      {canDistribute && (
        <>
          <div className={`${INSPECTOR_SECTION_HEADER_CLASS} mt-3`}>{t('menus.arrange.distribute')}</div>
          <div className="mt-1 flex gap-0.5">
            {DISTRIBUTIONS.map(({ direction, labelKey, icon }) => (
              <IconButton
                key={direction}
                size={COMPACT_BUTTON_SIZE}
                icon={icon}
                label={t(labelKey)}
                onClick={() => void distributeObjects(selectedObjectIds, direction)}
              />
            ))}
          </div>
        </>
      )}
    </section>
  );
}
