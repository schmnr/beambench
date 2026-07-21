import { useProjectStore } from '../stores/projectStore';
import { useUiStore } from '../stores/uiStore';
import { projectService } from '../services/projectService';
import type { CutEntry, CutEntryTemplate, Layer, Project, ProjectObject } from '../types/project';
import { expandArrangementSelectionMembers } from './arrangementSelection';

interface ObjectClipboard {
  objects: ProjectObject[];
  layers: Layer[];
}

let clipboard: ObjectClipboard | null = null;

export function getClipboard(): ProjectObject[] | null {
  return clipboard ? clipboard.objects : null;
}

export function hasClipboardData(): boolean {
  return clipboard !== null && clipboard.objects.length > 0;
}

export function clearClipboard(): void {
  clipboard = null;
  useUiStore.getState().setHasClipboard(false);
}

function cloneObjects(objects: ProjectObject[]): ProjectObject[] {
  return JSON.parse(JSON.stringify(objects)) as ProjectObject[];
}

function cloneLayers(layers: Layer[]): Layer[] {
  return JSON.parse(JSON.stringify(layers)) as Layer[];
}

function cutEntryTemplate(entry: CutEntry): CutEntryTemplate {
  return {
    operation: entry.operation,
    speed_mm_min: entry.speed_mm_min,
    power_percent: entry.power_percent,
    raster_settings: entry.raster_settings,
    vector_settings: entry.vector_settings,
    air_assist: entry.air_assist,
    power_min_percent: entry.power_min_percent,
    z_offset_mm: entry.z_offset_mm,
    gcode_prefix: entry.gcode_prefix,
    gcode_suffix: entry.gcode_suffix,
    output_enabled: entry.output_enabled,
  };
}

function normalizeColor(color: string): string {
  const normalized = color.trim().toLowerCase();
  return normalized.length === 9 && normalized.startsWith('#')
    ? normalized.slice(0, 7)
    : normalized;
}

function layerOperation(layer: Layer): CutEntry['operation'] {
  return layer.entries[0]?.operation ?? 'line';
}

function findCompatibleLayer(project: Project, template: Layer): Layer | null {
  const color = normalizeColor(template.color_tag);
  const operation = layerOperation(template);
  return project.layers.find(
    (layer) => normalizeColor(layer.color_tag) === color && layerOperation(layer) === operation,
  ) ?? null;
}

function selectedObjectSnapshots(objectIds: string[]): ObjectClipboard {
  const project = useProjectStore.getState().project;
  if (!project || objectIds.length === 0) return { objects: [], layers: [] };
  const snapshotIds = expandArrangementSelectionMembers(project, objectIds);
  const objects = cloneObjects(
    snapshotIds
      .map((id) => project.objects.find((object) => object.id === id) ?? null)
      .filter((object): object is ProjectObject => object !== null),
  );
  const layerIds = new Set(objects.map((object) => object.layer_id));
  const layers = cloneLayers(project.layers.filter((layer) => layerIds.has(layer.id)));
  return { objects, layers };
}

async function objectsForPaste(clip: ObjectClipboard): Promise<ProjectObject[]> {
  const objects = cloneObjects(clip.objects);
  const project = useProjectStore.getState().project;
  if (!project || objects.length === 0 || clip.layers.length === 0) {
    return objects;
  }

  const requiredLayerIds = new Set(objects.map((object) => object.layer_id));
  const layerIdMap = new Map<string, string>();
  const knownProject: Project = {
    ...project,
    layers: [...project.layers],
  };

  for (const template of clip.layers) {
    if (!requiredLayerIds.has(template.id)) continue;
    if (knownProject.layers.some((layer) => layer.id === template.id)) continue;

    const compatible = findCompatibleLayer(knownProject, template);
    if (compatible) {
      layerIdMap.set(template.id, compatible.id);
      continue;
    }

    let created = await projectService.addLayer(template.name, layerOperation(template));
    created = await projectService.updateLayer(created.id, {
      name: template.name,
      enabled: template.enabled,
      visible: template.visible,
      color_tag: template.color_tag,
    });

    if (!created.is_tool_layer && template.entries.length > 0) {
      created = await projectService.pasteLayerEntries(
        created.id,
        template.entries.map(cutEntryTemplate),
      );
    }

    knownProject.layers.push(created);
    layerIdMap.set(template.id, created.id);
  }

  if (layerIdMap.size === 0) return objects;
  return objects.map((object) => ({
    ...object,
    layer_id: layerIdMap.get(object.layer_id) ?? object.layer_id,
  }));
}

export async function clipboardCut(objectIds: string[]): Promise<void> {
  if (objectIds.length === 0) return;
  const snapshots = selectedObjectSnapshots(objectIds);
  if (snapshots.objects.length === 0) return;
  const removed = await useProjectStore.getState().removeObjects(snapshots.objects.map((object) => object.id));
  if (!removed) return;
  clipboard = snapshots;
  useUiStore.getState().setHasClipboard(true);
}

export function clipboardCopy(objectIds: string[]): void {
  clipboard = selectedObjectSnapshots(objectIds);
  useUiStore.getState().setHasClipboard(clipboard.objects.length > 0);
}

export async function clipboardPaste(): Promise<void> {
  const clip = clipboard;
  if (!clip || clip.objects.length === 0) return;
  await useProjectStore.getState().pasteObjects(await objectsForPaste(clip), false);
}

export async function clipboardPasteInPlace(): Promise<void> {
  const clip = clipboard;
  if (!clip || clip.objects.length === 0) return;
  await useProjectStore.getState().pasteObjects(await objectsForPaste(clip), true);
}

export async function clipboardDuplicate(objectIds: string[]): Promise<void> {
  if (objectIds.length === 0) return;
  await useProjectStore.getState().duplicateObjects(objectIds);
}
