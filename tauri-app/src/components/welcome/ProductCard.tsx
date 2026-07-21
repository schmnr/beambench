interface ProductCardProps {
  /** Product display name, e.g. "Craftgineer". */
  name: string;
  /** Short tagline shown under the name. */
  tagline: string;
  /** One-line description of what the product does. */
  description: string;
  /** Label for the call-to-action button. */
  buttonLabel: string;
  /** Opens the product site in the user's browser. */
  onVisit: () => void;
}

/**
 * A single equal-width promo card: name, tagline, one-line description, and a
 * call-to-action button. Text-only by design (no logo/image slot).
 */
export function ProductCard({ name, tagline, description, buttonLabel, onVisit }: ProductCardProps) {
  return (
    <div className="flex-1 min-w-0 flex flex-col bg-bb-surface border border-bb-border rounded-lg p-4">
      <div className="text-sm font-semibold text-bb-text">{name}</div>
      <div className="text-xs font-medium text-bb-accent mt-0.5">{tagline}</div>
      <p className="text-xs text-bb-text-muted mt-2 mb-3 flex-1 leading-relaxed">{description}</p>
      <button
        onClick={onVisit}
        className="bg-bb-accent text-bb-on-accent rounded px-4 py-1.5 text-sm font-medium hover:bg-bb-accent-hover transition-colors"
      >
        {buttonLabel}
      </button>
    </div>
  );
}
