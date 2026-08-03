export function WarpIcon({ size = 24 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <path d="M4 4 C9 2, 15 6, 20 4" stroke="currentColor" strokeWidth="1.6" />
      <path d="M4 12 C9 10, 15 14, 20 12" stroke="currentColor" strokeWidth="1.6" />
      <path d="M4 20 C9 18, 15 22, 20 20" stroke="currentColor" strokeWidth="1.6" />
      <path d="M4 4 C2 9, 6 15, 4 20" stroke="currentColor" strokeWidth="1.6" />
      <path d="M12 4 C10 9, 14 15, 12 20" stroke="currentColor" strokeWidth="1.6" />
      <path d="M20 4 C18 9, 22 15, 20 20" stroke="currentColor" strokeWidth="1.6" />
      <circle cx="4" cy="4" r="1.6" fill="rgb(var(--bb-accent))" />
      <circle cx="20" cy="4" r="1.6" fill="rgb(var(--bb-accent))" />
      <circle cx="4" cy="20" r="1.6" fill="rgb(var(--bb-accent))" />
      <circle cx="20" cy="20" r="1.6" fill="rgb(var(--bb-accent))" />
    </svg>
  );
}
