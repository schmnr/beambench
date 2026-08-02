import { useTranslation } from 'react-i18next';
import { useProjectStore } from '../../stores/projectStore';
import { TextInput } from '../shared/TextInput';
import { NumberInput } from '../shared/NumberInput';
import { TextPropertiesPanel } from './TextPropertiesPanel';
import { TransformSection } from './TransformSection';
import { RasterPropertiesPanel } from './RasterPropertiesPanel';
import { vectorService } from '../../services/vectorService';
import { useNotificationStore } from '../../stores/notificationStore';
import { wrapBackendError } from '../../i18n/errors';
import { RangeInput } from '../shared/RangeInput';
import { useUiStore } from '../../stores/uiStore';
import { TextDefaultsSection } from './TextDefaultsSection';
import { SelectionArrangeSection } from './SelectionArrangeSection';
import { NodeEditingSection } from './NodeEditingSection';
import { ModifierPropertiesSection } from './ModifierPropertiesSection';
import { RadiusPropertiesSection } from './RadiusPropertiesSection';
import { WarpPropertiesSection } from './WarpPropertiesSection';
import { TabPropertiesSection } from './TabPropertiesSection';
import { MeasurementPropertiesSection } from './MeasurementPropertiesSection';
import { createSelectionContext, isBooleanCompatible } from '../../commands/selectionContext';
import { IconButton } from '../shared/IconButton';
import { IconToggleButton } from '../shared/IconToggleButton';
import { Eye, EyeOff } from 'lucide-react';
import {
  ExcludeIcon,
  IntersectIcon,
  ReverseSubtractIcon,
  SubtractIcon,
  UnionIcon,
} from '../shared/BooleanOperationIcons';
import {
  INSPECTOR_CARD_CLASS,
  INSPECTOR_EMPTY_CLASS,
  INSPECTOR_SECTION_HEADER_CLASS,
} from '../shared/panelAppearance';

const TOAST_SUCCESS = 'success' as const;
const TOAST_ERROR = 'error' as const;
const BOOLEAN_BUTTON_SIZE = 'sm' as const;
const BOOLEAN_ICON_SIZE = 24;

export function PropertiesPanel() {
  const { t } = useTranslation();
  const project = useProjectStore((s) => s.project);
  const selectedObjectIds = useProjectStore((s) => s.selectedObjectIds);
  const updateObject = useProjectStore((s) => s.updateObject);
  const updateObjectData = useProjectStore((s) => s.updateObjectData);
  const loadProject = useProjectStore((s) => s.loadProject);
  const booleanPending = useProjectStore((s) => s.booleanPending);
  const setObjectsVisible = useProjectStore((s) => s.setObjectsVisible);
  const assignImageMask = useProjectStore((s) => s.assignImageMask);
  const activeTool = useUiStore((s) => s.activeTool);
  const modifierPropertiesSession = useUiStore((s) => s.modifierPropertiesSession);

  const selectedObject = project?.objects.find((o) => o.id === selectedObjectIds[0]) ?? null;
  // Multi-selection: batch controls and contextual arrange/vector operations.
  if (selectedObjectIds.length >= 2) {
    const selectedObjects = project?.objects.filter((o) => selectedObjectIds.includes(o.id)) ?? [];

    // Compute mixed state
    const allVisible = selectedObjects.every((o) => o.visible);
    const noneVisible = selectedObjects.every((o) => !o.visible);
    const allBooleanCompatible = selectedObjects.length === selectedObjectIds.length
      && selectedObjects.every((object) => isBooleanCompatible(object, project?.objects ?? []));
    const canUseBoolean = selectedObjectIds.length >= 2 && allBooleanCompatible && !booleanPending;
    const reverseBooleanOrder = [
      selectedObjectIds[selectedObjectIds.length - 1],
      ...selectedObjectIds.slice(0, -1),
    ];
    const selectionContext = createSelectionContext(
      selectedObjectIds,
      project?.objects ?? [],
      false,
      [],
    );
    const canApplyImageMask = selectionContext.canUseAsImageMask
      && !selectionContext.imageMaskSelectionHasInvalidMasks;

    return (
      <div className={INSPECTOR_CARD_CLASS} data-testid="properties-card">
      <div className="flex flex-col gap-2.5 px-3 py-2.5">
        <TransformSection />
        <div className="text-xs text-bb-text-dim">{t('panels.properties.objects_selected', { count: selectedObjectIds.length })}</div>

        {/* Batch controls */}
        <div className="flex items-center text-xs">
          <label className="flex items-center gap-1">
            <input
              type="checkbox"
              data-testid="batch-visible"
              checked={allVisible}
              ref={(el) => { if (el) el.indeterminate = !allVisible && !noneVisible; }}
              onChange={() => void setObjectsVisible(selectedObjectIds, !allVisible)}
            />
            {t('panels.properties.visible')}
          </label>
        </div>

        <SelectionArrangeSection />

        {selectionContext.canUseAsImageMask && (
          <section className="flex flex-col gap-1.5 border-t border-bb-border pt-3" data-testid="image-mask-section">
            <div className={INSPECTOR_SECTION_HEADER_CLASS}>{t('context_menu.use_as_image_mask')}</div>
            {selectionContext.imageMaskSelectionHasInvalidMasks && (
              <div className="text-[11px] text-bb-text-dim">
                {t('context_menu.image_mask_requires_closed')}
              </div>
            )}
            <div className="grid grid-cols-2 gap-1">
              <button
                type="button"
                className="h-8 rounded-lg border border-bb-border bg-bb-bg px-2 text-xs font-medium text-bb-text transition-colors hover:border-bb-accent/40 hover:bg-bb-hover disabled:cursor-default disabled:text-bb-text-disabled disabled:hover:border-bb-border disabled:hover:bg-bb-bg"
                disabled={!canApplyImageMask}
                onClick={() => void assignImageMask(
                  selectionContext.imageMaskTargetId!,
                  selectionContext.imageMaskObjectIds,
                  'keep_inside',
                )}
              >
                {t('panels.raster_properties.keep_inside')}
              </button>
              <button
                type="button"
                className="h-8 rounded-lg border border-bb-border bg-bb-bg px-2 text-xs font-medium text-bb-text transition-colors hover:border-bb-accent/40 hover:bg-bb-hover disabled:cursor-default disabled:text-bb-text-disabled disabled:hover:border-bb-border disabled:hover:bg-bb-bg"
                disabled={!canApplyImageMask}
                onClick={() => void assignImageMask(
                  selectionContext.imageMaskTargetId!,
                  selectionContext.imageMaskObjectIds,
                  'keep_outside',
                )}
              >
                {t('panels.raster_properties.keep_outside')}
              </button>
            </div>
          </section>
        )}

        {allBooleanCompatible && (
          <div className="flex flex-col gap-1.5 border-t border-bb-border pt-3">
            <div className={INSPECTOR_SECTION_HEADER_CLASS}>{t('panels.properties.boolean_operations')}</div>
            <div className="mt-1 grid grid-cols-5 gap-0.5">
              <IconButton
                icon={<UnionIcon size={BOOLEAN_ICON_SIZE} />}
                label={t('toolbars.modifiers.union')}
                onClick={() => void (selectedObjectIds.length === 2
                  ? useProjectStore.getState().booleanUnion(selectedObjectIds[0], selectedObjectIds[1])
                  : useProjectStore.getState().booleanUnionMany(selectedObjectIds))}
                disabled={!canUseBoolean}
                size={BOOLEAN_BUTTON_SIZE}
              />
              <IconButton
                icon={<SubtractIcon size={BOOLEAN_ICON_SIZE} />}
                label={`${t('toolbars.modifiers.subtract')} A − (B…)`}
                onClick={() => void (selectedObjectIds.length === 2
                  ? useProjectStore.getState().booleanSubtract(selectedObjectIds[0], selectedObjectIds[1])
                  : useProjectStore.getState().booleanSubtractMany(selectedObjectIds))}
                disabled={!canUseBoolean}
                size={BOOLEAN_BUTTON_SIZE}
              />
              <IconButton
                icon={<ReverseSubtractIcon size={BOOLEAN_ICON_SIZE} />}
                label={`${t('toolbars.modifiers.subtract')} B − (A…)`}
                onClick={() => void (selectedObjectIds.length === 2
                  ? useProjectStore.getState().booleanSubtract(selectedObjectIds[1], selectedObjectIds[0])
                  : useProjectStore.getState().booleanSubtractMany(reverseBooleanOrder))}
                disabled={!canUseBoolean}
                size={BOOLEAN_BUTTON_SIZE}
              />
              <IconButton
                icon={<IntersectIcon size={BOOLEAN_ICON_SIZE} />}
                label={t('toolbars.modifiers.intersect')}
                onClick={() => void (selectedObjectIds.length === 2
                  ? useProjectStore.getState().booleanIntersection(selectedObjectIds[0], selectedObjectIds[1])
                  : useProjectStore.getState().booleanIntersectionMany(selectedObjectIds))}
                disabled={!canUseBoolean}
                size={BOOLEAN_BUTTON_SIZE}
              />
              <IconButton
                icon={<ExcludeIcon size={BOOLEAN_ICON_SIZE} />}
                label={t('toolbars.modifiers.exclude')}
                onClick={() => void (selectedObjectIds.length === 2
                  ? useProjectStore.getState().booleanExclude(selectedObjectIds[0], selectedObjectIds[1])
                  : useProjectStore.getState().booleanExcludeMany(selectedObjectIds))}
                disabled={!canUseBoolean}
                size={BOOLEAN_BUTTON_SIZE}
              />
            </div>
          </div>
        )}

        {activeTool === 'node' && <NodeEditingSection />}
        {activeTool === 'radius' && <RadiusPropertiesSection />}
        {activeTool === 'warp' && <WarpPropertiesSection />}
        {activeTool === 'tabs' && <TabPropertiesSection />}
        {activeTool === 'measure' && <MeasurementPropertiesSection />}
        <ModifierPropertiesSection />
      </div>
      </div>
    );
  }

  if (!selectedObject) {
    if (activeTool === 'measure') {
      return (
        <div className={INSPECTOR_CARD_CLASS} data-testid="measurement-tool-card">
          <div className="p-3">
            <MeasurementPropertiesSection />
          </div>
        </div>
      );
    }
    if (activeTool === 'node') {
      return (
        <div className={INSPECTOR_CARD_CLASS} data-testid="node-editing-card">
          <div className="p-3">
            <NodeEditingSection />
          </div>
        </div>
      );
    }
    if (activeTool === 'text') {
      return (
        <div className={INSPECTOR_CARD_CLASS} data-testid="text-defaults-card">
          <TextDefaultsSection />
        </div>
      );
    }
    if (activeTool === 'radius') {
      return (
        <div className={INSPECTOR_CARD_CLASS} data-testid="radius-tool-card">
          <div className="p-3">
            <RadiusPropertiesSection />
          </div>
        </div>
      );
    }
    if (activeTool === 'warp') {
      return (
        <div className={INSPECTOR_CARD_CLASS} data-testid="warp-tool-card">
          <div className="p-3">
            <WarpPropertiesSection />
          </div>
        </div>
      );
    }
    if (activeTool === 'tabs') {
      return (
        <div className={INSPECTOR_CARD_CLASS} data-testid="tab-tool-card">
          <div className="p-3">
            <TabPropertiesSection />
          </div>
        </div>
      );
    }
    if (modifierPropertiesSession) {
      return (
        <div className={INSPECTOR_CARD_CLASS} data-testid="modifier-properties-card">
          <div className="p-3">
            <ModifierPropertiesSection />
          </div>
        </div>
      );
    }
    return <div className={INSPECTOR_EMPTY_CLASS}>{t('panels.properties.nothing_selected')}</div>;
  }

  const canConvertToPath =
    selectedObject.data?.type === 'shape' ||
    selectedObject.data?.type === 'text' ||
    selectedObject.data?.type === 'polygon' ||
    selectedObject.data?.type === 'star';

  // Corner radius: only for rectangle shapes
  const isRectangleShape = selectedObject.data?.type === 'shape' && selectedObject.data.kind === 'rectangle';
  const isEllipseShape = selectedObject.data?.type === 'shape' && selectedObject.data.kind === 'ellipse';
  const isTextObject = selectedObject.data?.type === 'text';
  const isPolygonShape = selectedObject.data?.type === 'polygon';
  const isStarShape = selectedObject.data?.type === 'star';
  const polygonData = isPolygonShape ? selectedObject.data as Extract<typeof selectedObject.data, { type: 'polygon' }> : null;
  const starData = isStarShape ? selectedObject.data as Extract<typeof selectedObject.data, { type: 'star' }> : null;
  const powerScalePercent = Math.round((selectedObject.power_scale ?? 1) * 100);

  return (
    <div className={INSPECTOR_CARD_CLASS} data-testid="properties-card">
    <div className="flex flex-col gap-2.5 px-3 py-2.5">
      <TransformSection />
      <TextInput
        label={t('panels.properties.name')}
        value={selectedObject.name}
        onChange={(name) => updateObject(selectedObject.id, { name })}
      />
      <div className="flex items-center justify-between gap-2 text-xs">
        <span className="text-bb-text-muted">{t('panels.layers.header.show')}</span>
        <IconToggleButton
          active={selectedObject.visible !== false}
          label={t('panels.layers.header.show')}
          icon={<Eye size={16} />}
          inactiveIcon={<EyeOff size={16} />}
          onClick={() => void setObjectsVisible([selectedObject.id], selectedObject.visible === false)}
          testId="object-show-toggle"
        />
      </div>
      <RangeInput
        label={t('panels.properties.power_scale_percent')}
        value={powerScalePercent}
        onChange={(v) => updateObject(selectedObject.id, { power_scale: v / 100 })}
        step={1}
        min={0}
        max={100}
        testId="properties-power-scale-slider"
      />

      <NumberInput
        label={t('panels.properties.cut_priority')}
        value={selectedObject.priority ?? 0}
        onChange={(v) => updateObject(selectedObject.id, { priority: v })}
        step={1}
        min={-99}
        max={99}
      />

      <SelectionArrangeSection />

      {isTextObject && selectedObject.data.type === 'text' && (
        <TextPropertiesPanel objectId={selectedObject.id} data={selectedObject.data} />
      )}

      {(isRectangleShape || isEllipseShape || isPolygonShape || isStarShape) && (
        <div className="flex flex-col gap-1.5 pt-1 border-t border-bb-border">
          <div className={INSPECTOR_SECTION_HEADER_CLASS}>{t('panels.properties.shape')}</div>
        </div>
      )}

      {isRectangleShape && selectedObject.data.type === 'shape' && (
        <NumberInput
          label={t('panels.properties.corner_radius')}
          value={selectedObject.data.corner_radius}
          onChange={(value) => {
            if (selectedObject.data.type === 'shape') {
              updateObjectData(selectedObject.id, { ...selectedObject.data, corner_radius: value });
            }
          }}
          step={0.5}
        />
      )}

      {polygonData && (
        <NumberInput
          label={t('panels.properties.sides')}
          value={polygonData.sides}
          onChange={(sides) => updateObjectData(selectedObject.id, { ...polygonData, sides: Math.max(3, Math.round(sides)) })}
          step={1}
          min={3}
        />
      )}

      {starData && (
        <div className="flex flex-col gap-1.5">
          <NumberInput
            label={t('panels.properties.points')}
            value={starData.points}
            onChange={(points) => updateObjectData(selectedObject.id, {
              ...starData,
              points: Math.max(3, Math.round(points)),
              corner_radii: [],
            })}
            step={1}
            min={3}
          />
          <NumberInput
            label={t('panels.properties.bulge')}
            value={starData.bulge}
            onChange={(bulge) => updateObjectData(selectedObject.id, { ...starData, bulge })}
            step={0.01}
            min={0}
            max={1}
          />
          <NumberInput
            label={t('panels.properties.ratio')}
            value={starData.ratio}
            onChange={(ratio) => updateObjectData(selectedObject.id, { ...starData, ratio })}
            step={0.01}
            min={0.05}
            max={0.95}
          />
          <label className="flex items-center gap-2 text-xs">
            <span className="text-bb-text-muted shrink-0">{t('panels.properties.dual_radius')}</span>
            <input
              type="checkbox"
              checked={starData.dual_radius}
              onChange={(e) => updateObjectData(selectedObject.id, {
                ...starData,
                dual_radius: e.target.checked,
                ratio2: e.target.checked ? (starData.ratio2 ?? 0.7) : null,
                corner_radii: [],
              })}
            />
          </label>
          {starData.dual_radius && (
            <NumberInput
              label={t('panels.properties.ratio_2')}
              value={starData.ratio2 ?? 0.7}
              onChange={(ratio2) => updateObjectData(selectedObject.id, { ...starData, ratio2 })}
              step={0.01}
              min={0.05}
              max={1}
            />
          )}
        </div>
      )}

      {selectedObject.data?.type === 'raster_image' && (
        <RasterPropertiesPanel
          objectId={selectedObject.id}
          data={selectedObject.data}
        />
      )}

      {canConvertToPath && (
        <button
          className="w-full text-xs px-2 py-1 rounded bg-bb-surface-2 border border-bb-border hover:bg-bb-surface-3 text-bb-text mt-1"
          onClick={() => {
            vectorService
              .convertToPath(selectedObject.id)
              .then(() => {
                loadProject({ invalidatePreview: true });
                useNotificationStore.getState().push(t('panels.properties.converted_to_path'), TOAST_SUCCESS);
              })
              .catch((err) => {
                useNotificationStore.getState().push(wrapBackendError(String(err)), TOAST_ERROR);
              });
          }}
        >
          {t('panels.properties.convert_to_path')}
        </button>
      )}

      {activeTool === 'node' && <NodeEditingSection />}
      {activeTool === 'radius' && <RadiusPropertiesSection />}
      {activeTool === 'warp' && <WarpPropertiesSection />}
      {activeTool === 'tabs' && <TabPropertiesSection />}
      {activeTool === 'measure' && <MeasurementPropertiesSection />}
      <ModifierPropertiesSection />
    </div>
    </div>
  );
}
