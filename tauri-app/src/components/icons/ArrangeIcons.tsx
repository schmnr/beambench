/**
 * Custom arrange-toolbar icons for Beam Bench.
 * Drawn at viewBox 0 0 24 24 with stroke=currentColor / strokeWidth=2 to match lucide-react.
 */

interface IconProps {
  size?: number;
  className?: string;
}

const baseProps = {
  fill: 'none' as const,
  stroke: 'currentColor',
  strokeWidth: 2,
  strokeLinecap: 'round' as const,
  strokeLinejoin: 'round' as const,
};

export function MirrorAcrossLineIcon({ size = 24, className }: IconProps) {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" width={size} height={size} viewBox="0 0 24 24" {...baseProps} className={className}>
      <line x1="3" y1="21" x2="21" y2="3" strokeDasharray="2 2" />
      <polygon points="5 19 13 19 5 11" />
      <polygon points="19 5 11 5 19 13" />
    </svg>
  );
}

export function MakeSameWidthIcon({ size = 24, className }: IconProps) {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" width={size} height={size} viewBox="0 0 24 24" {...baseProps} className={className}>
      {/* reference rect (wide) */}
      <rect x="3" y="4" width="18" height="5" rx="0.5" />
      {/* target rect (small) */}
      <rect x="9" y="14" width="6" height="5" rx="0.5" />
      {/* selection handles at target corners */}
      <rect x="8" y="13" width="1.5" height="1.5" fill="currentColor" stroke="none" />
      <rect x="14.5" y="13" width="1.5" height="1.5" fill="currentColor" stroke="none" />
      <rect x="8" y="19.5" width="1.5" height="1.5" fill="currentColor" stroke="none" />
      <rect x="14.5" y="19.5" width="1.5" height="1.5" fill="currentColor" stroke="none" />
      {/* widening arrows */}
      <path d="M3 16.5 L8 16.5" strokeWidth="1.5" />
      <path d="M5 15 L3 16.5 L5 18" strokeWidth="1.5" fill="none" />
      <path d="M21 16.5 L16 16.5" strokeWidth="1.5" />
      <path d="M19 15 L21 16.5 L19 18" strokeWidth="1.5" fill="none" />
    </svg>
  );
}

export function MakeSameHeightIcon({ size = 24, className }: IconProps) {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" width={size} height={size} viewBox="0 0 24 24" {...baseProps} className={className}>
      {/* reference rect (tall) */}
      <rect x="15" y="3" width="5" height="18" rx="0.5" />
      {/* target rect (small) */}
      <rect x="5" y="9" width="5" height="6" rx="0.5" />
      {/* selection handles at target corners */}
      <rect x="4" y="8" width="1.5" height="1.5" fill="currentColor" stroke="none" />
      <rect x="9.5" y="8" width="1.5" height="1.5" fill="currentColor" stroke="none" />
      <rect x="4" y="14.5" width="1.5" height="1.5" fill="currentColor" stroke="none" />
      <rect x="9.5" y="14.5" width="1.5" height="1.5" fill="currentColor" stroke="none" />
      {/* heightening arrows */}
      <path d="M7.5 3 L7.5 8" strokeWidth="1.5" />
      <path d="M6 5 L7.5 3 L9 5" strokeWidth="1.5" fill="none" />
      <path d="M7.5 21 L7.5 16" strokeWidth="1.5" />
      <path d="M6 19 L7.5 21 L9 19" strokeWidth="1.5" fill="none" />
    </svg>
  );
}

export function MoveHorizontallyTogetherIcon({ size = 24, className }: IconProps) {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" width={size} height={size} viewBox="0 0 24 24" {...baseProps} className={className}>
      {/* three vertical bars */}
      <rect x="2" y="6" width="3" height="12" rx="0.5" />
      <rect x="10.5" y="6" width="3" height="12" rx="0.5" />
      <rect x="19" y="6" width="3" height="12" rx="0.5" />
      {/* horizontal arrows showing edge-to-edge spacing */}
      <path d="M6 12 L9.5 12" strokeWidth="1.25" />
      <path d="M7.5 11 L6 12 L7.5 13" strokeWidth="1.25" fill="none" />
      <path d="M9 11 L10.5 12 L9 13" strokeWidth="1.25" fill="none" />
      <path d="M14.5 12 L18 12" strokeWidth="1.25" />
      <path d="M16 11 L14.5 12 L16 13" strokeWidth="1.25" fill="none" />
      <path d="M17.5 11 L19 12 L17.5 13" strokeWidth="1.25" fill="none" />
    </svg>
  );
}

export function MoveVerticallyTogetherIcon({ size = 24, className }: IconProps) {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" width={size} height={size} viewBox="0 0 24 24" {...baseProps} className={className}>
      {/* three horizontal bars */}
      <rect x="6" y="2" width="12" height="3" rx="0.5" />
      <rect x="6" y="10.5" width="12" height="3" rx="0.5" />
      <rect x="6" y="19" width="12" height="3" rx="0.5" />
      {/* vertical arrows showing edge-to-edge spacing */}
      <path d="M12 6 L12 9.5" strokeWidth="1.25" />
      <path d="M11 7.5 L12 6 L13 7.5" strokeWidth="1.25" fill="none" />
      <path d="M11 9 L12 10.5 L13 9" strokeWidth="1.25" fill="none" />
      <path d="M12 14.5 L12 18" strokeWidth="1.25" />
      <path d="M11 16 L12 14.5 L13 16" strokeWidth="1.25" fill="none" />
      <path d="M11 17.5 L12 19 L13 17.5" strokeWidth="1.25" fill="none" />
    </svg>
  );
}

export function ResizeSlotsIcon({ size = 24, className }: IconProps) {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" width={size} height={size} viewBox="0 0 24 24" {...baseProps} className={className}>
      {/* slotted bar — outer outline showing 2 slots cut into top edge */}
      <path d="M2 18 L2 12 L7 12 L7 6 L11 6 L11 12 L13 12 L13 6 L17 6 L17 12 L22 12 L22 18 Z" />
      {/* slot-depth arrow inside the second slot */}
      <path d="M15 8.5 L15 11.5" strokeWidth="1.25" />
      <path d="M14 9.5 L15 8.5 L16 9.5" strokeWidth="1.25" fill="none" />
      <path d="M14 10.5 L15 11.5 L16 10.5" strokeWidth="1.25" fill="none" />
    </svg>
  );
}

export function DockToEdgeIcon({ size = 24, className }: IconProps) {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" width={size} height={size} viewBox="0 0 24 24" {...baseProps} className={className}>
      {/* page outline (dashed) */}
      <rect x="3" y="3" width="18" height="18" rx="0.5" strokeDasharray="2 2" />
      {/* docked object snapped to top-left corner */}
      <rect x="3" y="3" width="9" height="6" rx="0.5" fill="currentColor" fillOpacity="0.25" />
      {/* arrow showing dock direction */}
      <path d="M16 16 L13 13" strokeWidth="1.5" />
      <path d="M13.5 14.5 L13 13 L14.5 13.5" strokeWidth="1.5" fill="none" />
    </svg>
  );
}
