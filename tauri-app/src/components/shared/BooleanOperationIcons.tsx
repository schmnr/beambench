interface BooleanOperationIconProps {
  size?: number;
}

export const UnionIcon = ({ size = 24 }: BooleanOperationIconProps) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" stroke="none" aria-hidden="true">
    <path d="M1,1 H15 V9 H23 V23 H9 V15 H1 Z" />
  </svg>
);

export const SubtractIcon = ({ size = 24 }: BooleanOperationIconProps) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" aria-hidden="true">
    <path d="M1,1 H15 V9 H9 V15 H1 Z" fill="currentColor" stroke="none" />
    <rect x="9" y="9" width="14" height="14" strokeWidth="1.5" strokeDasharray="2.5 2" />
  </svg>
);

export const ReverseSubtractIcon = ({ size = 24 }: BooleanOperationIconProps) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" aria-hidden="true">
    <rect x="1" y="1" width="14" height="14" strokeWidth="1.5" strokeDasharray="2.5 2" />
    <path d="M15,9 H23 V23 H9 V15 H15 Z" fill="currentColor" stroke="none" />
  </svg>
);

export const IntersectIcon = ({ size = 24 }: BooleanOperationIconProps) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" aria-hidden="true">
    <rect x="1" y="1" width="14" height="14" strokeWidth="1.5" strokeDasharray="2.5 2" />
    <rect x="9" y="9" width="14" height="14" strokeWidth="1.5" strokeDasharray="2.5 2" />
    <rect x="9" y="9" width="6" height="6" fill="currentColor" stroke="none" />
  </svg>
);

export const ExcludeIcon = ({ size = 24 }: BooleanOperationIconProps) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" aria-hidden="true">
    <rect x="1" y="1" width="14" height="14" strokeWidth="1.5" strokeDasharray="2.5 2" />
    <rect x="9" y="9" width="14" height="14" strokeWidth="1.5" strokeDasharray="2.5 2" />
    <path d="M1,1 H15 V9 H9 V15 H1 Z" fill="currentColor" stroke="none" />
    <path d="M15,9 H23 V23 H9 V15 H15 Z" fill="currentColor" stroke="none" />
  </svg>
);

export const WeldIcon = ({ size = 24 }: BooleanOperationIconProps) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" aria-hidden="true">
    <path d="M10,1 H23 V14 H19 V23 H5 V18 H1 V5 H10 Z" strokeWidth="1.5" />
    <line x1="10" y1="5" x2="14" y2="5" strokeWidth="1" strokeDasharray="1.5 1.5" />
    <line x1="10" y1="5" x2="10" y2="14" strokeWidth="1" strokeDasharray="1.5 1.5" />
    <line x1="14" y1="5" x2="14" y2="18" strokeWidth="1" strokeDasharray="1.5 1.5" />
    <line x1="5" y1="12" x2="19" y2="12" strokeWidth="1" strokeDasharray="1.5 1.5" />
    <line x1="5" y1="18" x2="14" y2="18" strokeWidth="1" strokeDasharray="1.5 1.5" />
    <line x1="10" y1="14" x2="19" y2="14" strokeWidth="1" strokeDasharray="1.5 1.5" />
    <line x1="5" y1="12" x2="5" y2="18" strokeWidth="1" strokeDasharray="1.5 1.5" />
    <line x1="19" y1="12" x2="19" y2="14" strokeWidth="1" strokeDasharray="1.5 1.5" />
  </svg>
);
