import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { invoke } from '@tauri-apps/api/core';
import { save, open } from '@tauri-apps/plugin-dialog';

import { useMaterialStore } from '../../stores/materialStore.js';
import { useProjectStore } from '../../stores/projectStore.js';
import { useNotificationStore } from '../../stores/notificationStore.js';
import { wrapBackendError } from '../../i18n/errors';
import { useAppStore } from '../../stores/appStore.js';
import type { MaterialPreset } from '../../types/material.js';
import type { OperationType } from '../../types/project.js';
import { effectiveDpi, effectiveLineIntervalMm } from '../../types/rasterSettings.js';
import {
  displaySpeedToMmMin,
  formatSpeedForDisplay,
  speedUnitLabel,
} from '../../utils/speedUnits.js';
import {
  mmToDisplay,
  displayToMm,
  roundDisplayLength,
  lengthUnitLabel,
  labelWithUnit,
} from '../../utils/lengthUnits';

const OPERATION_OPTIONS: Array<{ value: OperationType; labelKey: string }> = [
  { value: 'line', labelKey: 'panels.machine.material_library.operation.line' },
  { value: 'fill', labelKey: 'panels.machine.material_library.operation.fill' },
  { value: 'offset_fill', labelKey: 'panels.machine.material_library.operation.offset_fill' },
];

const OPERATION_LABEL_KEYS: Record<OperationType, string> = {
  image: 'panels.machine.material_library.operation.image',
  fill: 'panels.machine.material_library.operation.fill',
  offset_fill: 'panels.machine.material_library.operation.offset_fill',
  cut: 'panels.machine.material_library.operation.cut',
  score: 'panels.machine.material_library.operation.score',
  tool: 'panels.machine.material_library.operation.tool',
  line: 'panels.machine.material_library.operation.line',
};

function operationLabel(operation: OperationType, t: (key: string) => string): string {
  return t(OPERATION_LABEL_KEYS[operation] ?? OPERATION_LABEL_KEYS.line);
}

function operationOptionsForEdit(
  operation: OperationType,
  t: (key: string, options?: Record<string, string>) => string,
): Array<{ value: OperationType; label: string }> {
  if (OPERATION_OPTIONS.some((option) => option.value === operation)) {
    return OPERATION_OPTIONS.map((option) => ({
      value: option.value,
      label: t(option.labelKey),
    }));
  }

  return [
    ...OPERATION_OPTIONS.map((option) => ({
      value: option.value,
      label: t(option.labelKey),
    })),
    {
      value: operation,
      label: t('panels.machine.material_library.operation_existing', {
        operation: operationLabel(operation, t),
      }),
    },
  ];
}

export function MaterialLibrary() {
  const { t } = useTranslation();
  const presets = useMaterialStore((s) => s.presets);
  const loadPresets = useMaterialStore((s) => s.loadPresets);
  const savePreset = useMaterialStore((s) => s.savePreset);
  const deletePreset = useMaterialStore((s) => s.deletePreset);
  const applyPreset = useMaterialStore((s) => s.applyPreset);
  const selectedLayerId = useProjectStore((s) => s.selectedLayerId);
  const project = useProjectStore((s) => s.project);
  const appSettings = useAppStore((s) => s.settings);
  const displayUnit = appSettings?.display_unit === 'inches' ? 'inches' : 'mm';
  const speedTimeUnit = appSettings?.speed_time_unit === 'seconds' ? 'seconds' : 'minutes';
  const speedLabel = speedUnitLabel(displayUnit, speedTimeUnit);
  const formatThickness = (mm: number) => String(roundDisplayLength(mmToDisplay(mm, displayUnit), displayUnit));

  // Use explicit selection, or fall back to first layer (matches rest of app)
  const effectiveLayerId = selectedLayerId ?? project?.layers[0]?.id ?? null;

  const [search, setSearch] = useState('');
  const [opFilter, setOpFilter] = useState<string>('all');
  const [deviceFilter, setDeviceFilter] = useState<string>('all');
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editingPreset, setEditingPreset] = useState<MaterialPreset | null>(null);
  const [editName, setEditName] = useState('');
  const [editMaterial, setEditMaterial] = useState('');
  const [editThickness, setEditThickness] = useState('');
  const [editOperation, setEditOperation] = useState<OperationType>('line');
  const [editSpeed, setEditSpeed] = useState('1000');
  const [editPower, setEditPower] = useState('50');
  const [editPasses, setEditPasses] = useState('1');
  const [editNotes, setEditNotes] = useState('');
  const [pendingEditPreset, setPendingEditPreset] = useState<MaterialPreset | null>(null);

  // When the unit setting changes mid-edit, reinterpret the current display
  // strings from the previous unit and reformat them in the new unit, so an
  // in-progress edit is not silently misread on save.
  const prevDisplayUnit = useRef<'mm' | 'inches'>(displayUnit);
  const prevSpeedTimeUnit = useRef<'minutes' | 'seconds'>(speedTimeUnit);
  useEffect(() => {
    if (editingPreset === null) {
      prevDisplayUnit.current = displayUnit;
      prevSpeedTimeUnit.current = speedTimeUnit;
      return;
    }
    setEditThickness((cur) => formatThickness(displayToMm(Number(cur) || 0, prevDisplayUnit.current)));
    setEditSpeed((cur) =>
      formatSpeedForDisplay(
        displaySpeedToMmMin(Number(cur) || 0, prevDisplayUnit.current, prevSpeedTimeUnit.current),
        displayUnit,
        speedTimeUnit,
      ),
    );
    prevDisplayUnit.current = displayUnit;
    prevSpeedTimeUnit.current = speedTimeUnit;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [displayUnit, speedTimeUnit]);

  useEffect(() => {
    loadPresets();
  }, [loadPresets]);

  // Collect unique device profiles for filter dropdown
  const deviceProfiles = [...new Set((presets ?? []).map((p) => p.device_profile).filter(Boolean))] as string[];

  const filtered = (presets ?? []).filter(
    (p) =>
      (p.name.toLowerCase().includes(search.toLowerCase()) ||
      p.material.toLowerCase().includes(search.toLowerCase())) &&
      (opFilter === 'all' || p.operation === opFilter) &&
      (deviceFilter === 'all' || (p.device_profile ?? '') === deviceFilter || (!p.device_profile && deviceFilter === 'all')),
  );

  // Group by material, then sub-group by thickness
  const grouped = new Map<string, Map<string, MaterialPreset[]>>();
  for (const p of filtered) {
    const matKey = p.material || t('panels.machine.material_library.uncategorized');
    if (!grouped.has(matKey)) grouped.set(matKey, new Map());
    const thickKey = p.thickness_mm != null ? `${formatThickness(p.thickness_mm)}${lengthUnitLabel(displayUnit)}` : t('panels.machine.material_library.unspecified');
    const thickMap = grouped.get(matKey)!;
    const arr = thickMap.get(thickKey) ?? [];
    arr.push(p);
    thickMap.set(thickKey, arr);
  }

  function handleAdd(): void {
    savePreset({
      id: crypto.randomUUID(),
      name: t('panels.machine.material_library.new_preset'),
      material: t('panels.machine.material_library.custom_material'),
      thickness_mm: 3.0,
      operation: 'line',
      speed_mm_min: 1000,
      power_percent: 50,
      passes: 1,
      notes: '',
      category: '',
    });
  }

  const hasUnsavedChanges = editingPreset !== null && (
      editName !== editingPreset.name ||
      editMaterial !== editingPreset.material ||
      editThickness !== formatThickness(editingPreset.thickness_mm) ||
      editOperation !== editingPreset.operation ||
      editSpeed !== formatSpeedForDisplay(editingPreset.speed_mm_min, displayUnit, speedTimeUnit) ||
      editPower !== String(editingPreset.power_percent) ||
      editPasses !== String(editingPreset.passes) ||
      editNotes !== (editingPreset.notes ?? '')
    );

  function beginEditing(preset: MaterialPreset): void {
    setPendingEditPreset(null);
    setEditingId(preset.id);
    setEditingPreset(preset);
    setEditName(preset.name);
    setEditMaterial(preset.material);
    setEditThickness(formatThickness(preset.thickness_mm));
    setEditOperation(preset.operation);
    setEditSpeed(formatSpeedForDisplay(preset.speed_mm_min, displayUnit, speedTimeUnit));
    setEditPower(String(preset.power_percent));
    setEditPasses(String(preset.passes));
    setEditNotes(preset.notes ?? '');
  }

  function startEditing(preset: MaterialPreset): void {
    if (editingId !== null && editingId !== preset.id && hasUnsavedChanges) {
      setPendingEditPreset(preset);
      return;
    }

    beginEditing(preset);
  }

  async function handleSaveEdit() {
    if (editingId == null || editingPreset == null) return;
    const nextSpeed = Number(editSpeed);
    const originalSpeedDisplay = formatSpeedForDisplay(
      editingPreset.speed_mm_min,
      displayUnit,
      speedTimeUnit,
    );

    const saved = await savePreset({
      ...editingPreset,
      id: editingId,
      name: editName,
      material: editMaterial,
      thickness_mm: Number.isFinite(Number(editThickness)) && Number(editThickness) > 0
        ? displayToMm(Number(editThickness), displayUnit)
        : 3.0,
      operation: editOperation,
      speed_mm_min: editSpeed === originalSpeedDisplay
        ? editingPreset.speed_mm_min
        : Number.isFinite(nextSpeed) && nextSpeed > 0
          ? displaySpeedToMmMin(nextSpeed, displayUnit, speedTimeUnit)
          : 1000,
      power_percent: Number(editPower) || 50,
      passes: Number(editPasses) || 1,
      notes: editNotes,
    });
    if (saved) {
      setEditingId(null);
      setEditingPreset(null);
    }
  }

  function handleCancelEdit(): void {
    setEditingId(null);
    setEditingPreset(null);
  }

  function handleDuplicate(preset: MaterialPreset): void {
    savePreset({
      ...preset,
      id: crypto.randomUUID(),
      name: t('panels.machine.material_library.copy_name', { name: preset.name }),
    });
  }

  async function handleExport() {
    try {
      const path = await save({
        filters: [{ name: 'JSON', extensions: ['json'] }],
        defaultPath: 'material-presets.json',
      });
      if (!path) return;
      await invoke('export_material_presets', { path });
      useNotificationStore.getState().push(t('panels.machine.material_library.presets_exported'), 'success');
    } catch (e) {
      useNotificationStore.getState().push(wrapBackendError(String(e)), 'error');
    }
  }

  async function handleImport() {
    try {
      const selected = await open({
        filters: [{ name: 'JSON', extensions: ['json'] }],
        multiple: false,
      });
      if (selected === null || Array.isArray(selected)) return;
      const path = selected;
      await invoke<MaterialPreset[]>('import_material_presets', { path });
      await loadPresets();
      useNotificationStore.getState().push(t('panels.machine.material_library.presets_imported'), 'success');
    } catch (e) {
      useNotificationStore.getState().push(wrapBackendError(String(e)), 'error');
    }
  }

  return (
    <div className="px-2 pb-2 space-y-1">
      {pendingEditPreset && (
        <div className="rounded border border-bb-warning-border bg-bb-warning-bg p-2 text-xs text-bb-text">
          <div className="mb-2 text-bb-warning-fg">
            {t('panels.machine.material_library.discard_unsaved_changes')}
          </div>
          <div className="flex justify-end gap-2">
            <button
              className="rounded border border-bb-border px-2 py-0.5 text-bb-text-muted hover:bg-bb-hover"
              onClick={() => setPendingEditPreset(null)}
            >
              {t('dialog.machine_profile.keep_editing')}
            </button>
            <button
              className="rounded bg-bb-warning px-2 py-0.5 font-medium text-bb-on-warning hover:bg-bb-warning-hover"
              onClick={() => {
                const preset = pendingEditPreset;
                if (preset) beginEditing(preset);
              }}
            >
              {t('dialog.machine_profile.discard')}
            </button>
          </div>
        </div>
      )}
      <input
        placeholder={t('panels.machine.material_library.search_placeholder')}
        className="w-full px-1.5 py-0.5 bg-bb-bg border border-bb-border rounded text-xs text-bb-text focus:outline-none focus:border-bb-accent"
        value={search}
        onChange={(e) => setSearch(e.target.value)}
      />
      <div className="flex gap-1 items-center">
        <select
          className="px-1 py-0.5 bg-bb-bg border border-bb-border rounded text-xs text-bb-text"
          value={opFilter}
          onChange={(e) => setOpFilter(e.target.value)}
          data-testid="operation-filter"
        >
          <option value="all">{t('panels.machine.material_library.all_ops')}</option>
          {OPERATION_OPTIONS.map((opt) => (
            <option key={opt.value} value={opt.value}>{t(opt.labelKey)}</option>
          ))}
        </select>
        {deviceProfiles.length > 0 && (
          <select
            className="px-1 py-0.5 bg-bb-bg border border-bb-border rounded text-xs text-bb-text"
            value={deviceFilter}
            onChange={(e) => setDeviceFilter(e.target.value)}
            data-testid="device-filter"
          >
            <option value="all">{t('panels.machine.material_library.all_devices')}</option>
            {deviceProfiles.map((d) => (
              <option key={d} value={d}>{d}</option>
            ))}
          </select>
        )}
      </div>
      <div className="flex gap-1">
        <button
          className="flex-1 px-2 py-1 text-xs bg-bb-accent text-bb-on-accent rounded hover:bg-bb-accent-hover"
          onClick={handleAdd}
        >
          {t('panels.machine.material_library.add')}
        </button>
        <button
          data-testid="import-presets"
          className="px-2 py-1 text-xs bg-bb-bg border border-bb-border text-bb-text rounded hover:bg-bb-hover"
          onClick={() => void handleImport()}
        >
          {t('panels.machine.material_library.import')}
        </button>
        <button
          data-testid="export-presets"
          className="px-2 py-1 text-xs bg-bb-bg border border-bb-border text-bb-text rounded hover:bg-bb-hover"
          onClick={() => void handleExport()}
        >
          {t('panels.machine.material_library.export')}
        </button>
        <button
          data-testid="create-from-layer"
          className="px-2 py-1 text-xs bg-bb-bg border border-bb-border text-bb-text rounded hover:bg-bb-hover"
          disabled={!effectiveLayerId}
          onClick={() => {
            const layer = project?.layers.find((l) => l.id === effectiveLayerId);
            if (!layer) return;
            const entry = layer.entries[0];
            if (!entry) return;
            // For image layers write both dpi and line_interval_mm as derived-consistent
            // values via the shared effective helpers. For non-image layers write
            // only line_interval_mm and leave dpi null (it is unused there).
            const rs = entry.raster_settings ?? null;
            const isImage = entry.operation === 'image';
            const dpiOut = isImage ? effectiveDpi(rs) : (rs?.dpi ?? null);
            const liOut = isImage
              ? effectiveLineIntervalMm(rs)
              : (rs?.line_interval_mm ?? null);
            savePreset({
              id: crypto.randomUUID(),
              name: t('panels.machine.material_library.layer_preset_name', {
                layer: layer.name || t('panels.machine.material_library.layer'),
              }),
              material: t('panels.machine.material_library.custom_material'),
              thickness_mm: 3.0,
              operation: entry.operation,
              speed_mm_min: entry.speed_mm_min,
              power_percent: entry.power_percent,
              passes: entry.vector_settings?.passes ?? entry.raster_settings?.passes ?? 1,
              dpi: dpiOut,
              raster_mode: isImage ? (rs?.mode ?? null) : null,
              line_interval_mm: liOut,
              scan_angle: rs?.scan_angle ?? null,
              bidirectional: rs?.bidirectional ?? null,
              overscan_mm: rs?.overscan_mm ?? null,
              flood_fill: rs?.flood_fill ?? null,
              angle_passes: rs?.angle_passes ?? null,
              angle_increment_deg: rs?.angle_increment_deg ?? null,
              pass_through: isImage ? (rs?.pass_through ?? null) : null,
              halftone_cells_per_inch: isImage ? (rs?.halftone_cells_per_inch ?? null) : null,
              halftone_angle_deg: isImage ? (rs?.halftone_angle_deg ?? null) : null,
              newsprint_angle_deg: isImage ? (rs?.newsprint_angle_deg ?? null) : null,
              newsprint_frequency: isImage ? (rs?.newsprint_frequency ?? null) : null,
              // Layer-level image fields (Phase C1)
              invert: isImage ? (rs?.invert ?? null) : null,
              dot_width_correction_mm: isImage ? (rs?.dot_width_correction_mm ?? null) : null,
              ramp_length_mm: isImage ? (rs?.ramp_length_mm ?? null) : null,
              notes: '',
              category: '',
            });
          }}
        >
          {t('panels.machine.material_library.from_layer')}
        </button>
      </div>

      {[...grouped.entries()].map(([material, thicknessMap]) => (
        <div key={material} data-testid="material-group">
          <div className="text-xs font-medium text-bb-text-muted px-1 py-0.5 bg-bb-bg-alt border-b border-bb-border mt-1">
            {material}
          </div>
          {[...thicknessMap.entries()].map(([thickness, groupPresets]) => (
          <div key={thickness} data-testid="thickness-group">
            {thicknessMap.size > 1 && (
              <div className="text-[10px] text-bb-text-dim px-2 py-0.5 bg-bb-bg-alt/50">
                {thickness}
              </div>
            )}
          {groupPresets.map((preset) =>
            editingId === preset.id ? (
              <div
                key={preset.id}
                className="p-1 bg-bb-bg border border-bb-border rounded text-xs space-y-1"
              >
                <input
                  placeholder={t('panels.machine.material_library.name')}
                  className="w-full px-1 py-0.5 bg-bb-bg border border-bb-border rounded text-xs text-bb-text"
                  value={editName}
                  onChange={(e) => setEditName(e.target.value)}
                />
                <input
                  placeholder={t('panels.machine.material_library.material')}
                  className="w-full px-1 py-0.5 bg-bb-bg border border-bb-border rounded text-xs text-bb-text"
                  value={editMaterial}
                  onChange={(e) => setEditMaterial(e.target.value)}
                />
                <input
                  placeholder={labelWithUnit(t('panels.machine.material_library.thickness_mm'), lengthUnitLabel(displayUnit))}
                  className="w-full px-1 py-0.5 bg-bb-bg border border-bb-border rounded text-xs text-bb-text"
                  value={editThickness}
                  onChange={(e) => setEditThickness(e.target.value)}
                />
                <select
                  className="w-full px-1 py-0.5 bg-bb-bg border border-bb-border rounded text-xs text-bb-text"
                  value={editOperation}
                  onChange={(e) => setEditOperation(e.target.value as OperationType)}
                  data-testid="operation-select"
                >
                  {operationOptionsForEdit(editOperation, t).map((opt) => (
                    <option key={opt.value} value={opt.value}>{opt.label}</option>
                  ))}
                </select>
                <div className="grid grid-cols-3 gap-1">
                  <input
                    placeholder={t('panels.machine.material_library.speed_with_unit', { unit: speedLabel })}
                    className="px-1 py-0.5 bg-bb-bg border border-bb-border rounded text-xs text-bb-text"
                    value={editSpeed}
                    onChange={(e) => setEditSpeed(e.target.value)}
                  />
                  <input
                    placeholder={t('panels.machine.material_library.power_percent')}
                    className="px-1 py-0.5 bg-bb-bg border border-bb-border rounded text-xs text-bb-text"
                    value={editPower}
                    onChange={(e) => setEditPower(e.target.value)}
                  />
                  <input
                    placeholder={t('panels.machine.material_library.passes')}
                    className="px-1 py-0.5 bg-bb-bg border border-bb-border rounded text-xs text-bb-text"
                    value={editPasses}
                    onChange={(e) => setEditPasses(e.target.value)}
                  />
                </div>
                <input
                  placeholder={t('panels.machine.material_library.notes')}
                  className="w-full px-1 py-0.5 bg-bb-bg border border-bb-border rounded text-xs text-bb-text"
                  value={editNotes}
                  onChange={(e) => setEditNotes(e.target.value)}
                />
                <div className="flex gap-1">
                  <button
                    className="px-2 py-0.5 bg-bb-accent text-bb-on-accent rounded text-xs hover:bg-bb-accent-hover"
                    onClick={() => void handleSaveEdit()}
                  >
                    {t('common.save')}
                  </button>
                  <button
                    className="px-2 py-0.5 bg-bb-bg border border-bb-border rounded text-xs hover:bg-bb-accent-hover"
                    onClick={handleCancelEdit}
                  >
                    {t('common.cancel')}
                  </button>
                </div>
              </div>
            ) : (
              <div
                key={preset.id}
                className="flex items-center gap-1 p-1 bg-bb-bg border border-bb-border rounded text-xs"
              >
                <div className="flex-1 truncate">
                  <div className="font-medium">{preset.name}</div>
                  <div className="text-bb-text-muted">
                    {preset.material}
                    {preset.thickness_mm != null ? ` - ${formatThickness(preset.thickness_mm)}${lengthUnitLabel(displayUnit)}` : ''}
                    {' | '}{operationLabel(preset.operation, t)} {formatSpeedForDisplay(preset.speed_mm_min, displayUnit, speedTimeUnit)} {speedLabel} {preset.power_percent}%
                  </div>
                </div>
                <button
                  title={t('common.apply')}
                  className="px-1 hover:text-bb-accent"
                  disabled={!effectiveLayerId}
                  onClick={() => applyPreset(preset.id, effectiveLayerId!)}
                >
                  +
                </button>
                <button
                  title={t('panels.machine.material_library.duplicate')}
                  className="px-1 hover:text-bb-accent"
                  onClick={() => handleDuplicate(preset)}
                >
                  D
                </button>
                <button
                  title={t('panels.machine.material_library.edit')}
                  className="px-1 hover:text-bb-accent"
                  onClick={() => startEditing(preset)}
                >
                  E
                </button>
                <button
                  title={t('panels.machine.material_library.delete')}
                  className="px-1 hover:text-bb-error-fg"
                  onClick={() => deletePreset(preset.id)}
                >
                  X
                </button>
              </div>
            ),
          )}
          </div>
          ))}
        </div>
      ))}
    </div>
  );
}
