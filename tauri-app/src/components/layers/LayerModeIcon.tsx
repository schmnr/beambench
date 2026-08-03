import type { OperationType } from '../../types/project';

interface LayerModeIconProps {
  operation: OperationType;
  size?: number;
  className?: string;
  testId?: string;
}

/**
 * Compact, programmatic mode marks used anywhere a layer is identified.
 * Each silhouette remains distinct at the 12–16 px sizes used by tabs and
 * inspectors, without relying on a text suffix in the layer name.
 */
export function LayerModeIcon({
  operation,
  size = 14,
  className,
  testId,
}: LayerModeIconProps) {
  const shared = {
    width: size,
    height: size,
    viewBox: '0 0 16 16',
    fill: 'none',
    xmlns: 'http://www.w3.org/2000/svg',
    className,
    'aria-hidden': true,
    'data-testid': testId,
    'data-operation': operation,
  } as const;

  switch (operation) {
    case 'fill':
      return (
        <svg {...shared}>
          <path d="M3 3.25h10v9.5H3z" fill="currentColor" />
        </svg>
      );
    case 'offset_fill':
      return (
        <svg {...shared}>
          <rect x="2.25" y="2.25" width="11.5" height="11.5" rx="2" stroke="currentColor" strokeWidth="1.35" />
          <rect x="4.75" y="4.75" width="6.5" height="6.5" rx="1" stroke="currentColor" strokeWidth="1.25" />
          <rect x="7" y="7" width="2" height="2" rx="0.45" fill="currentColor" />
        </svg>
      );
    case 'image':
      return (
        <svg {...shared}>
          <rect x="2.25" y="2.25" width="11.5" height="11.5" rx="1" stroke="currentColor" strokeWidth="1.25" />
          <path d="M3 3h4v4H3zm6 0h4v4H9zM3 9h4v4H3zm6 0h4v4H9z" fill="currentColor" />
          <path d="M7 3h2v4H7zM7 9h2v4H7zM3 7h4v2H3zm6 0h4v2H9z" fill="currentColor" opacity="0.28" />
        </svg>
      );
    case 'tool':
      return (
        <svg {...shared}>
          <path d="M3 5V3h2M11 3h2v2M13 11v2h-2M5 13H3v-2" stroke="currentColor" strokeWidth="1.35" strokeLinecap="round" />
          <path d="M5.25 8h5.5M8 5.25v5.5" stroke="currentColor" strokeWidth="1.25" strokeLinecap="round" />
          <circle cx="8" cy="8" r="1.15" stroke="currentColor" strokeWidth="1.15" />
        </svg>
      );
    case 'cut':
    case 'score':
    case 'line':
      return (
        <svg {...shared}>
          <path d="M2.5 11.75 5.75 5.5l3.1 3.05L13.5 3.9" stroke="currentColor" strokeWidth="1.55" strokeLinecap="round" strokeLinejoin="round" />
          <circle cx="2.5" cy="11.75" r="1" fill="currentColor" />
          <circle cx="13.5" cy="3.9" r="1" fill="currentColor" />
        </svg>
      );
  }
}
