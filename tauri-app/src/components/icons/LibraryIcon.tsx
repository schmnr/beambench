interface LibraryIconProps {
  size?: number;
  className?: string;
}

/**
 * Archive-style library icon. The stepped handles suggest a collection of
 * stored items while the solid front bin keeps the mark distinct from arrays.
 */
export function LibraryIcon({ size = 24, className }: LibraryIconProps) {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      className={className}
      aria-hidden="true"
      data-icon="library-archive"
    >
      <path
        d="M7.25 7.25V6.4A2.9 2.9 0 0 1 10.15 3.5h3.7a2.9 2.9 0 0 1 2.9 2.9v.85"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
        opacity="0.45"
      />
      <path
        d="M5.25 9V8.15A2.65 2.65 0 0 1 7.9 5.5h8.2a2.65 2.65 0 0 1 2.65 2.65V9"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
        opacity="0.7"
      />
      <path
        d="M4.35 8.25h15.3c1.25 0 2.2 1.13 1.98 2.36l-1.27 7.13a3.4 3.4 0 0 1-3.35 2.81H6.99a3.4 3.4 0 0 1-3.35-2.81l-1.27-7.13a2.02 2.02 0 0 1 1.98-2.36Z"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        d="M9.3 15.8h5.4"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
      />
    </svg>
  );
}
