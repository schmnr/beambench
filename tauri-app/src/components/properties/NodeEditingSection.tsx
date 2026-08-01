import { useState, type ReactNode } from 'react';
import { ChevronDown, ChevronRight } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import type { NodeImmediateAction } from '../../canvas/tools/NodeTool';
import { useProjectStore } from '../../stores/projectStore';
import { useUiStore, type NodeSubMode } from '../../stores/uiStore';

const ICON_SIZE = 18;

function IconShell({ children }: { children: ReactNode }) {
  return (
    <svg
      width={ICON_SIZE}
      height={ICON_SIZE}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.75"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {children}
    </svg>
  );
}

function NodeSquare({ x, y, accent = false }: { x: number; y: number; accent?: boolean }) {
  return (
    <rect
      x={x - 2}
      y={y - 2}
      width="4"
      height="4"
      rx="0.5"
      fill="currentColor"
      stroke="none"
      className={accent ? 'text-bb-accent' : undefined}
    />
  );
}

function NodeGlyph() {
  return (
    <svg
      className="h-3.5 w-3.5"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      aria-hidden="true"
    >
      <path d="M4 18C8 6 16 6 20 18" />
      <rect x="2.5" y="16.5" width="4" height="4" rx="0.5" fill="currentColor" />
      <rect x="17.5" y="16.5" width="4" height="4" rx="0.5" fill="currentColor" />
      <circle cx="12" cy="7.6" r="2" fill="none" />
    </svg>
  );
}

const SelectMoveIcon = () => (
  <IconShell>
    <path d="m4.5 3.5 6.6 16 2.35-6.9 6.9-2.35Z" fill="currentColor" strokeWidth="1.5" />
    <NodeSquare x={19} y={19} accent />
  </IconShell>
);

const InsertNodeIcon = () => (
  <IconShell>
    <path d="M3 19.5 Q12 12.5 21 19.5" opacity="0.45" />
    <NodeSquare x={3} y={19.5} />
    <NodeSquare x={21} y={19.5} />
    <g className="text-bb-accent" stroke="currentColor" strokeWidth="2.25">
      <line x1="12" y1="3.5" x2="12" y2="12.5" />
      <line x1="7.5" y1="8" x2="16.5" y2="8" />
    </g>
  </IconShell>
);

const InsertMidpointIcon = () => (
  <IconShell>
    <path d="M4 19.5 L20 19.5" opacity="0.45" />
    <NodeSquare x={4} y={19.5} />
    <NodeSquare x={20} y={19.5} />
    <NodeSquare x={12} y={19.5} accent />
    <g className="text-bb-accent" stroke="currentColor" strokeWidth="2.25">
      <line x1="12" y1="3.5" x2="12" y2="12.5" />
      <line x1="7.5" y1="8" x2="16.5" y2="8" />
    </g>
  </IconShell>
);

const BreakAtNodeIcon = () => (
  <IconShell>
    <path d="M2.5 20 Q6 13 8.5 12" opacity="0.45" />
    <path d="M15.5 12 Q18 13 21.5 20" opacity="0.45" />
    <NodeSquare x={8} y={12} accent />
    <NodeSquare x={16} y={12} accent />
    <g className="text-bb-accent" stroke="currentColor" strokeWidth="1.9">
      <path d="M6.5 5.5 L3.5 8.5 L6.5 11.5" />
      <path d="M17.5 5.5 L20.5 8.5 L17.5 11.5" />
    </g>
  </IconShell>
);

const DeleteNodeIcon = () => (
  <IconShell>
    <path d="M3 19.5 Q12 12.5 21 19.5" opacity="0.45" />
    <NodeSquare x={3} y={19.5} />
    <NodeSquare x={12} y={16} accent />
    <NodeSquare x={21} y={19.5} />
    <g className="text-bb-accent" stroke="currentColor" strokeWidth="2.25">
      <line x1="7.5" y1="8" x2="16.5" y2="8" />
    </g>
  </IconShell>
);

const DeleteSegmentIcon = () => (
  <IconShell>
    <path d="M3 19.5 Q12 12.5 21 19.5" opacity="0.45" strokeDasharray="2.5 2" />
    <NodeSquare x={3} y={19.5} />
    <NodeSquare x={21} y={19.5} />
    <g className="text-bb-accent" stroke="currentColor" strokeWidth="2.25">
      <line x1="7.5" y1="8" x2="16.5" y2="8" />
    </g>
  </IconShell>
);

const ToLineIcon = () => (
  <IconShell>
    <path d="M4 12.5 Q12 2.5 20 12.5" opacity="0.45" strokeDasharray="2.5 2" />
    <path d="M4 18 L20 18" className="text-bb-accent" stroke="currentColor" strokeWidth="2" />
    <NodeSquare x={4} y={18} />
    <NodeSquare x={20} y={18} />
  </IconShell>
);

const ToSmoothIcon = () => (
  <IconShell>
    <path d="M3 17 C8 5.5 16 5.5 21 17" className="text-bb-accent" stroke="currentColor" strokeWidth="2" />
    <line x1="5" y1="8" x2="19" y2="8" opacity="0.55" strokeWidth="1.5" />
    <circle cx="5" cy="8" r="1.5" fill="currentColor" stroke="none" />
    <circle cx="19" cy="8" r="1.5" fill="currentColor" stroke="none" />
    <NodeSquare x={12} y={8} />
  </IconShell>
);

const ToCornerIcon = () => (
  <IconShell>
    <path d="M4 18.5 L12 5.5 L20 18.5" className="text-bb-accent" stroke="currentColor" strokeWidth="2" />
    <NodeSquare x={12} y={5.5} />
  </IconShell>
);

const ExtendEndpointIcon = () => (
  <IconShell>
    <path d="M3 20.5 L9 14.5" opacity="0.45" />
    <NodeSquare x={9.5} y={14} />
    <g className="text-bb-accent" stroke="currentColor" strokeWidth="2">
      <line x1="12.5" y1="11" x2="19.5" y2="4" />
      <path d="M13.5 4 L19.5 4 L19.5 10" />
    </g>
  </IconShell>
);

const ClosePathIcon = () => (
  <IconShell>
    <path d="M16 5.07 A8 8 0 1 0 19.88 10.6" />
    <NodeSquare x={16} y={5} />
    <NodeSquare x={19.9} y={10.6} />
    <path d="M16 5.07 L19.88 10.6" className="text-bb-accent" stroke="currentColor" strokeWidth="2" strokeDasharray="2.5 2" />
  </IconShell>
);

interface NodeModeDefinition {
  mode: NodeSubMode;
  labelKey: string;
  icon: ReactNode;
}

const NODE_MODE_GROUPS: readonly (readonly NodeModeDefinition[])[] = [
  [{ mode: 'select', labelKey: 'toolbars.node_sub.select_move', icon: <SelectMoveIcon /> }],
  [
    { mode: 'insert', labelKey: 'toolbars.node_sub.insert_node', icon: <InsertNodeIcon /> },
    { mode: 'insert_midpoint', labelKey: 'toolbars.node_sub.insert_midpoint', icon: <InsertMidpointIcon /> },
  ],
  [
    { mode: 'break', labelKey: 'toolbars.node_sub.break_at_node', icon: <BreakAtNodeIcon /> },
    { mode: 'delete_node', labelKey: 'toolbars.node_sub.delete_node', icon: <DeleteNodeIcon /> },
    { mode: 'delete_segment', labelKey: 'toolbars.node_sub.delete_segment', icon: <DeleteSegmentIcon /> },
  ],
  [
    { mode: 'to_line', labelKey: 'toolbars.node_sub.convert_to_line', icon: <ToLineIcon /> },
    { mode: 'to_smooth', labelKey: 'toolbars.node_sub.convert_to_smooth', icon: <ToSmoothIcon /> },
    { mode: 'to_corner', labelKey: 'toolbars.node_sub.convert_to_corner', icon: <ToCornerIcon /> },
  ],
  [{ mode: 'extend', labelKey: 'toolbars.node_sub.extend_to_intersection', icon: <ExtendEndpointIcon /> }],
];

const NODE_MODES = NODE_MODE_GROUPS.flat();

function dispatchImmediateAction(action: NodeImmediateAction) {
  window.dispatchEvent(new CustomEvent('bb:node-immediate-action', { detail: action }));
}

export function NodeEditingSection() {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(true);
  const nodeSubMode = useUiStore((state) => state.nodeSubMode);
  const nodeEditOpenPathObjectId = useUiStore((state) => state.nodeEditOpenPathObjectId);
  const setNodeSubMode = useUiStore((state) => state.setNodeSubMode);
  const project = useProjectStore((state) => state.project);
  const selectedObjectIds = useProjectStore((state) => state.selectedObjectIds);
  const selectedObject = selectedObjectIds.length === 1
    ? project?.objects.find((object) => object.id === selectedObjectIds[0]) ?? null
    : null;
  const activeMode = NODE_MODES.find((mode) => mode.mode === nodeSubMode) ?? NODE_MODES[0];
  const canClosePath = selectedObject?.data.type === 'vector_path'
    && nodeEditOpenPathObjectId === selectedObject.id
    && !selectedObject.locked;

  return (
    <section
      data-testid="node-editing-section"
      className="overflow-hidden rounded-lg border border-bb-border bg-bb-bg/40"
    >
      <button
        type="button"
        aria-expanded={expanded}
        aria-label={t('toolbars.creation.node_edit')}
        onClick={() => setExpanded((current) => !current)}
        className={`flex h-9 w-full items-center gap-2 px-3 text-left outline-none transition-colors hover:bg-bb-hover focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-bb-accent ${
          expanded ? 'border-b border-bb-border bg-gradient-to-r from-bb-accent/10 to-bb-surface/30' : 'bg-bb-surface/30'
        }`}
      >
        <span className="text-bb-accent"><NodeGlyph /></span>
        <span className="text-[10px] font-semibold uppercase tracking-[0.16em] text-bb-text-dim">
          {t('toolbars.creation.node_edit')}
        </span>
        {expanded
          ? <ChevronDown className="ml-auto h-3.5 w-3.5 text-bb-text-dim" />
          : <ChevronRight className="ml-auto h-3.5 w-3.5 text-bb-text-dim" />}
      </button>

      {expanded && (
        <div className="flex flex-col gap-2.5 p-3">
          <div className="flex flex-wrap gap-1.5" role="toolbar" aria-label={t('toolbars.creation.node_edit')}>
            {NODE_MODE_GROUPS.map((group) => (
              <div
                key={group[0].mode}
                className="flex overflow-hidden rounded-lg border border-bb-border bg-bb-surface/70"
              >
                {group.map((mode, index) => {
                  const label = t(mode.labelKey);
                  const active = nodeSubMode === mode.mode;
                  return (
                    <button
                      key={mode.mode}
                      type="button"
                      aria-label={label}
                      aria-pressed={active}
                      title={label}
                      onClick={() => setNodeSubMode(mode.mode)}
                      className={`flex h-[34px] w-[38px] items-center justify-center outline-none transition-colors focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-bb-accent ${
                        index > 0 ? 'border-l border-bb-border' : ''
                      } ${
                        active
                          ? 'bg-bb-accent/15 text-bb-accent'
                          : 'text-bb-text-muted hover:bg-bb-hover hover:text-bb-text'
                      }`}
                    >
                      {mode.icon}
                    </button>
                  );
                })}
              </div>
            ))}
          </div>

          <div className="flex items-center gap-2.5 rounded-lg border border-bb-border bg-bb-panel px-2.5 py-2">
            <span className="shrink-0 text-bb-text-muted">{activeMode.icon}</span>
            <span className="text-[11px] font-semibold text-bb-text">{t(activeMode.labelKey)}</span>
          </div>

          <div className="border-t border-bb-border pt-2.5">
            <button
              type="button"
              disabled={!canClosePath}
              onClick={() => dispatchImmediateAction('close_open')}
              className="flex h-8 w-full items-center justify-start gap-2 rounded-lg border border-bb-border bg-bb-surface px-2 text-[11px] font-medium text-bb-text outline-none transition-colors hover:bg-bb-hover focus-visible:ring-1 focus-visible:ring-bb-accent disabled:cursor-default disabled:text-bb-text-disabled disabled:hover:bg-bb-surface"
            >
              <ClosePathIcon />
              {t('toolbars.node_sub.close_path')}
            </button>
          </div>
        </div>
      )}
    </section>
  );
}
