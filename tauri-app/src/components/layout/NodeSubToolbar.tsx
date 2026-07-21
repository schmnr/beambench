import { useTranslation } from 'react-i18next';
import { useUiStore, type NodeSubMode } from '../../stores/uiStore';
import { IconButton } from '../shared/IconButton';
import { MousePointer2 } from 'lucide-react';
import type { NodeImmediateAction } from '../../canvas/tools/NodeTool';

interface SubToolDef {
  mode: NodeSubMode;
  icon: React.ReactNode;
  labelKey: string;
}

const NODE_ICON_SIZE = 20;
const NODE_BUTTON_SIZE = 'sm' as const;
const NODE_MODE_CLOSE_OPEN = 'close_open' as const;
const NODE_MODE_AUTO_JOIN = 'auto_join' as const;
const NODE_MODE_INSERT_MIDPOINT = 'insert_midpoint' as const;
const NODE_MODE_ALIGN = 'align' as const;
const NODE_ACTION_CLOSE_OPEN: NodeImmediateAction = 'close_open';
const NODE_ACTION_AUTO_JOIN: NodeImmediateAction = 'auto_join';
const NODE_ACTION_MIDPOINT: NodeImmediateAction = 'midpoint';
const NODE_ACTION_ALIGN: NodeImmediateAction = 'align';

const InsertNodeIcon = ({ size = 20 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
    <path d="M3 17 Q12 3 21 17" />
    <circle cx="12" cy="9" r="3" fill="rgb(34,197,94)" stroke="rgb(34,197,94)" strokeWidth="1.5" />
    <line x1="12" y1="7" x2="12" y2="11" stroke="white" strokeWidth="1.5" />
    <line x1="10" y1="9" x2="14" y2="9" stroke="white" strokeWidth="1.5" />
  </svg>
);

const DeleteNodeIcon = ({ size = 20 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
    <path d="M3 17 Q12 3 21 17" />
    <circle cx="12" cy="9" r="3" fill="rgb(239,68,68)" stroke="rgb(239,68,68)" strokeWidth="1.5" />
    <line x1="10" y1="9" x2="14" y2="9" stroke="white" strokeWidth="1.5" />
  </svg>
);

const BreakAtNodeIcon = ({ size = 20 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
    <path d="M3 17 Q7 8 11 10" />
    <path d="M13 10 Q17 8 21 17" />
    <rect x="9.5" y="7.5" width="5" height="5" rx="0.5" fill="rgb(34,192,238)" />
  </svg>
);

const DeleteSegmentIcon = ({ size = 20 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
    <path d="M3 17 L9 10" />
    <path d="M15 10 L21 17" strokeDasharray="3 2" stroke="rgb(239,68,68)" />
    <rect x="7" y="8" width="4" height="4" rx="0.5" fill="currentColor" />
    <rect x="13" y="8" width="4" height="4" rx="0.5" fill="currentColor" />
  </svg>
);

const ToLineIcon = ({ size = 20 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
    <line x1="4" y1="18" x2="20" y2="6" />
    <rect x="2" y="16" width="4" height="4" rx="0.5" fill="currentColor" />
    <rect x="18" y="4" width="4" height="4" rx="0.5" fill="currentColor" />
  </svg>
);

const ToSmoothIcon = ({ size = 20 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
    <path d="M3 18 C8 4 16 4 21 18" />
    <circle cx="12" cy="7" r="2.5" fill="rgb(34,192,238)" stroke="rgb(34,192,238)" />
  </svg>
);

const ToCornerIcon = ({ size = 20 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
    <path d="M4 18 L12 6 L20 18" />
    <rect x="10" y="4" width="4" height="4" rx="0.5" fill="rgb(34,192,238)" />
  </svg>
);

const CloseOpenIcon = ({ size = 20 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
    <path d="M4 16 Q8 4 12 8 Q16 12 20 4" />
    <path d="M4 16 L20 4" strokeDasharray="3 2" stroke="rgb(34,197,94)" />
  </svg>
);

const AutoJoinIcon = ({ size = 20 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
    <path d="M3 16 Q8 4 12 12" />
    <path d="M12 12 Q16 20 21 8" />
    <circle cx="12" cy="12" r="2.5" fill="rgb(34,197,94)" stroke="rgb(34,197,94)" />
  </svg>
);

const InsertMidpointIcon = ({ size = 20 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
    <path d="M3 17 Q12 3 21 17" />
    <rect x="1" y="15" width="4" height="4" rx="0.5" fill="currentColor" />
    <rect x="19" y="15" width="4" height="4" rx="0.5" fill="currentColor" />
    <circle cx="12" cy="9" r="3" fill="rgb(34,197,94)" stroke="rgb(34,197,94)" strokeWidth="1.5" />
    <line x1="12" y1="7" x2="12" y2="11" stroke="white" strokeWidth="1.5" />
    <line x1="10" y1="9" x2="14" y2="9" stroke="white" strokeWidth="1.5" />
  </svg>
);

const AlignIcon = ({ size = 20 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
    <line x1="3" y1="19" x2="21" y2="19" strokeDasharray="2.5 2" stroke="rgb(34,192,238)" strokeWidth="1.5" />
    <line x1="5" y1="19" x2="19" y2="5" />
    <rect x="3" y="17" width="4" height="4" rx="0.5" fill="currentColor" />
    <rect x="17" y="3" width="4" height="4" rx="0.5" fill="currentColor" />
  </svg>
);

function GroupSeparator() {
  return <div className="w-9 h-px bg-bb-border my-0.5" />;
}

function ActionSubModeButton({
  label,
  shortcut,
  icon,
  action,
  mode,
  active,
  setNodeSubMode,
}: {
  label: string;
  shortcut?: string;
  icon?: React.ReactNode;
  action: NodeImmediateAction;
  mode: NodeSubMode;
  active: boolean;
  setNodeSubMode: (mode: NodeSubMode) => void;
}) {
  return (
    <IconButton
      icon={icon ?? <span className="text-[10px] font-semibold">{shortcut}</span>}
      label={label}
      onClick={() => {
        setNodeSubMode(mode);
        window.dispatchEvent(
          new CustomEvent('bb:node-immediate-action', {
            detail: action,
          }),
        );
      }}
      active={active}
      size={NODE_BUTTON_SIZE}
    />
  );
}

const SUB_TOOLS: (SubToolDef | 'sep')[] = [
  { mode: 'select', icon: <MousePointer2 size={NODE_ICON_SIZE} />, labelKey: 'toolbars.node_sub.select_move' },
  'sep',
  { mode: 'insert', icon: <InsertNodeIcon size={NODE_ICON_SIZE} />, labelKey: 'toolbars.node_sub.insert_node' },
  { mode: 'delete_node', icon: <DeleteNodeIcon size={NODE_ICON_SIZE} />, labelKey: 'toolbars.node_sub.delete_node' },
  'sep',
  { mode: 'break', icon: <BreakAtNodeIcon size={NODE_ICON_SIZE} />, labelKey: 'toolbars.node_sub.break_at_node' },
  { mode: 'delete_segment', icon: <DeleteSegmentIcon size={NODE_ICON_SIZE} />, labelKey: 'toolbars.node_sub.delete_segment' },
  'sep',
  { mode: 'to_line', icon: <ToLineIcon size={NODE_ICON_SIZE} />, labelKey: 'toolbars.node_sub.convert_to_line' },
  { mode: 'to_smooth', icon: <ToSmoothIcon size={NODE_ICON_SIZE} />, labelKey: 'toolbars.node_sub.convert_to_smooth' },
  { mode: 'to_corner', icon: <ToCornerIcon size={NODE_ICON_SIZE} />, labelKey: 'toolbars.node_sub.convert_to_corner' },
];

export function NodeSubToolbar() {
  const { t } = useTranslation();
  const activeTool = useUiStore((s) => s.activeTool);
  const nodeSubMode = useUiStore((s) => s.nodeSubMode);
  const setNodeSubMode = useUiStore((s) => s.setNodeSubMode);

  if (activeTool !== 'node') return null;

  return (
    <div className="no-select w-12 bg-bb-panel py-1 gap-0.5 text-xs border-r border-bb-border flex flex-col items-center">
      {SUB_TOOLS.map((item, i) =>
        item === 'sep' ? (
          <GroupSeparator key={`sep-${i}`} />
        ) : (
          <IconButton
            key={item.mode}
            icon={item.icon}
            label={t(item.labelKey)}
            onClick={() => setNodeSubMode(item.mode)}
            active={nodeSubMode === item.mode}
            size={NODE_BUTTON_SIZE}
          />
        ),
      )}
      <GroupSeparator />
      <ActionSubModeButton
        action={NODE_ACTION_CLOSE_OPEN}
        mode={NODE_MODE_CLOSE_OPEN}
        icon={<CloseOpenIcon size={NODE_ICON_SIZE} />}
        label={t('toolbars.node_sub.close_path')}
        active={nodeSubMode === 'close_open'}
        setNodeSubMode={setNodeSubMode}
      />
      <ActionSubModeButton
        action={NODE_ACTION_AUTO_JOIN}
        mode={NODE_MODE_AUTO_JOIN}
        icon={<AutoJoinIcon size={NODE_ICON_SIZE} />}
        label={t('toolbars.node_sub.auto_join_paths')}
        active={nodeSubMode === 'auto_join'}
        setNodeSubMode={setNodeSubMode}
      />
      <GroupSeparator />
      <ActionSubModeButton
        action={NODE_ACTION_MIDPOINT}
        mode={NODE_MODE_INSERT_MIDPOINT}
        icon={<InsertMidpointIcon size={NODE_ICON_SIZE} />}
        label={t('toolbars.node_sub.insert_midpoint')}
        active={nodeSubMode === 'insert_midpoint'}
        setNodeSubMode={setNodeSubMode}
      />
      <ActionSubModeButton
        action={NODE_ACTION_ALIGN}
        mode={NODE_MODE_ALIGN}
        icon={<AlignIcon size={NODE_ICON_SIZE} />}
        label={t('toolbars.node_sub.align_to_angle')}
        active={nodeSubMode === 'align'}
        setNodeSubMode={setNodeSubMode}
      />
    </div>
  );
}
