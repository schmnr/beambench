export function TabsIcon({ size = 24 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <circle
        cx="12"
        cy="12"
        r="9"
        stroke="currentColor"
        strokeWidth="1.8"
        strokeDasharray="5 3.5"
      />
      <rect x="11" y="1.5" width="2" height="3.5" rx="0.5" fill="rgb(var(--bb-accent))" />
      <rect x="11" y="19" width="2" height="3.5" rx="0.5" fill="rgb(var(--bb-accent))" />
      <rect x="1.5" y="11" width="3.5" height="2" rx="0.5" fill="rgb(var(--bb-accent))" />
      <rect x="19" y="11" width="3.5" height="2" rx="0.5" fill="rgb(var(--bb-accent))" />
    </svg>
  );
}
