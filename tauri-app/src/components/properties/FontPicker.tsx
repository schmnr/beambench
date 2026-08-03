import { useEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { Check, ChevronDown, Clock3, Search, Star } from 'lucide-react';

const RECENT_FONTS_KEY = 'beambench.recent-fonts';
const FAVORITE_FONTS_KEY = 'beambench.favorite-fonts';
const MAX_RECENT_FONTS = 6;
const MAX_VISIBLE_FONTS = 120;
const STAR_FILLED = 'currentColor';
const STAR_EMPTY = 'none';

interface FontPickerProps {
  label: string;
  value: string;
  fonts: string[];
  onChange: (font: string) => void;
  searchLabel: string;
  recentLabel: string;
  favoritesLabel: string;
  allFontsLabel: string;
  noFontsFoundLabel: string;
  favoriteFontLabel: string;
  unfavoriteFontLabel: string;
  disabled?: boolean;
}

function readStoredFonts(key: string): string[] {
  try {
    const parsed = JSON.parse(localStorage.getItem(key) ?? '[]');
    return Array.isArray(parsed) ? parsed.filter((item): item is string => typeof item === 'string') : [];
  } catch {
    return [];
  }
}

function storeFonts(key: string, fonts: string[]) {
  try {
    localStorage.setItem(key, JSON.stringify(fonts));
  } catch {
    // Font preferences are a convenience; private storage modes may reject writes.
  }
}

function uniqueFonts(fonts: string[]): string[] {
  const seen = new Set<string>();
  return fonts.filter((font) => {
    const key = font.toLocaleLowerCase();
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

export function FontPicker({
  label,
  value,
  fonts,
  onChange,
  searchLabel,
  recentLabel,
  favoritesLabel,
  allFontsLabel,
  noFontsFoundLabel,
  favoriteFontLabel,
  unfavoriteFontLabel,
  disabled,
}: FontPickerProps) {
  const buttonRef = useRef<HTMLButtonElement>(null);
  const popoverRef = useRef<HTMLDivElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const [activeIndex, setActiveIndex] = useState(0);
  const [recentFonts, setRecentFonts] = useState<string[]>(() => readStoredFonts(RECENT_FONTS_KEY));
  const [favoriteFonts, setFavoriteFonts] = useState<string[]>(() => readStoredFonts(FAVORITE_FONTS_KEY));
  const [position, setPosition] = useState({ left: 0, top: 0, width: 240 });

  const availableFonts = useMemo(() => uniqueFonts([value, ...fonts]), [fonts, value]);
  const filteredFonts = useMemo(() => {
    const trimmed = query.trim().toLocaleLowerCase();
    const matching = trimmed
      ? availableFonts.filter((font) => font.toLocaleLowerCase().includes(trimmed))
      : availableFonts;
    const prioritized = trimmed
      ? matching
      : uniqueFonts([
          ...favoriteFonts.filter((font) => matching.includes(font)),
          ...recentFonts.filter((font) => matching.includes(font)),
          ...matching,
        ]);
    return prioritized.slice(0, MAX_VISIBLE_FONTS);
  }, [availableFonts, favoriteFonts, query, recentFonts]);

  const reposition = () => {
    const rect = buttonRef.current?.getBoundingClientRect();
    if (!rect) return;
    const width = Math.max(240, rect.width);
    const roomBelow = window.innerHeight - rect.bottom;
    const estimatedHeight = 330;
    setPosition({
      left: Math.min(rect.left, Math.max(8, window.innerWidth - width - 8)),
      top: roomBelow >= estimatedHeight || rect.top < estimatedHeight
        ? rect.bottom + 4
        : Math.max(8, rect.top - estimatedHeight - 4),
      width,
    });
  };

  useEffect(() => {
    if (!open) return;
    reposition();
    setQuery('');
    setActiveIndex(Math.max(0, filteredFonts.indexOf(value)));
    requestAnimationFrame(() => searchRef.current?.focus());

    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node;
      if (!buttonRef.current?.contains(target) && !popoverRef.current?.contains(target)) setOpen(false);
    };
    const onViewportChange = () => reposition();
    document.addEventListener('pointerdown', onPointerDown);
    window.addEventListener('resize', onViewportChange);
    window.addEventListener('scroll', onViewportChange, true);
    return () => {
      document.removeEventListener('pointerdown', onPointerDown);
      window.removeEventListener('resize', onViewportChange);
      window.removeEventListener('scroll', onViewportChange, true);
    };
    // Opening is the only event that should reset the search field.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  useEffect(() => {
    setActiveIndex((index) => Math.min(index, Math.max(0, filteredFonts.length - 1)));
  }, [filteredFonts.length]);

  const chooseFont = (font: string) => {
    onChange(font);
    const nextRecent = uniqueFonts([font, ...recentFonts]).slice(0, MAX_RECENT_FONTS);
    setRecentFonts(nextRecent);
    storeFonts(RECENT_FONTS_KEY, nextRecent);
    setOpen(false);
  };

  const toggleFavorite = (font: string) => {
    const nextFavorites = favoriteFonts.includes(font)
      ? favoriteFonts.filter((item) => item !== font)
      : uniqueFonts([...favoriteFonts, font]).sort((a, b) => a.localeCompare(b));
    setFavoriteFonts(nextFavorites);
    storeFonts(FAVORITE_FONTS_KEY, nextFavorites);
  };

  const sectionLabel = (index: number) => {
    if (query.trim()) return null;
    const favorites = filteredFonts.filter((item) => favoriteFonts.includes(item));
    const recent = filteredFonts.filter((item) => !favoriteFonts.includes(item) && recentFonts.includes(item));
    if (index === 0 && favorites.length > 0) return favoritesLabel;
    if (index === favorites.length && recent.length > 0) return recentLabel;
    if (index === favorites.length + recent.length) return allFontsLabel;
    return null;
  };

  return (
    <label className="flex items-center justify-between gap-2 text-xs">
      <span className="shrink-0 text-bb-text-muted">{label}</span>
      <button
        ref={buttonRef}
        type="button"
        disabled={disabled}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={`${label}: ${value}`}
        onClick={() => setOpen((current) => !current)}
        className="flex h-7 w-40 min-w-0 items-center justify-between gap-2 rounded border border-bb-control-border bg-bb-input px-2 text-left text-xs text-bb-text hover:bg-bb-hover focus:border-bb-accent disabled:opacity-60"
      >
        <span className="truncate" style={{ fontFamily: value }}>{value}</span>
        <ChevronDown size={13} className="shrink-0 text-bb-text-dim" />
      </button>

      {open && createPortal(
        <div
          ref={popoverRef}
          className="fixed z-[80] overflow-hidden rounded-lg border border-bb-control-border bg-bb-surface-elevated shadow-lg"
          style={{ left: position.left, top: position.top, width: position.width }}
        >
          <div className="border-b border-bb-border p-2">
            <div className="flex h-8 items-center gap-2 rounded border border-bb-control-border bg-bb-input px-2 focus-within:border-bb-accent">
              <Search size={14} className="shrink-0 text-bb-text-dim" />
              <input
                ref={searchRef}
                role="combobox"
                aria-expanded="true"
                aria-controls="font-picker-options"
                aria-autocomplete="list"
                aria-activedescendant={filteredFonts[activeIndex] ? `font-option-${activeIndex}` : undefined}
                value={query}
                placeholder={searchLabel}
                onChange={(event) => {
                  setQuery(event.target.value);
                  setActiveIndex(0);
                }}
                onKeyDown={(event) => {
                  if (event.key === 'Escape') {
                    event.preventDefault();
                    setOpen(false);
                    buttonRef.current?.focus();
                  } else if (event.key === 'ArrowDown') {
                    event.preventDefault();
                    setActiveIndex((index) => Math.min(filteredFonts.length - 1, index + 1));
                  } else if (event.key === 'ArrowUp') {
                    event.preventDefault();
                    setActiveIndex((index) => Math.max(0, index - 1));
                  } else if (event.key === 'Enter' && filteredFonts[activeIndex]) {
                    event.preventDefault();
                    chooseFont(filteredFonts[activeIndex]);
                  }
                }}
                className="min-w-0 flex-1 bg-transparent text-xs text-bb-text outline-none placeholder:text-bb-text-muted"
              />
            </div>
          </div>

          <div id="font-picker-options" role="listbox" className="max-h-[274px] overflow-y-auto p-1 scrollbar-safe-edge">
            {filteredFonts.map((font, index) => {
              const favorite = favoriteFonts.includes(font);
              const heading = sectionLabel(index);
              return (
                <div key={font}>
                  {heading ? (
                    <div className="flex items-center gap-1.5 px-2 pb-1 pt-2 text-[10px] font-semibold text-bb-text-dim">
                      {heading === favoritesLabel ? <Star size={11} /> : heading === recentLabel ? <Clock3 size={11} /> : null}
                      {heading}
                    </div>
                  ) : null}
                  <div
                    id={`font-option-${index}`}
                    role="option"
                    aria-selected={font === value}
                    className={`group flex min-h-9 items-center rounded px-2 ${index === activeIndex ? 'bg-bb-hover' : ''}`}
                    onMouseEnter={() => setActiveIndex(index)}
                  >
                    <button
                      type="button"
                      className="flex min-w-0 flex-1 items-center gap-2 text-left"
                      onClick={() => chooseFont(font)}
                    >
                      <span className="w-4 shrink-0 text-bb-accent">{font === value ? <Check size={14} /> : null}</span>
                      <span className="truncate text-sm text-bb-text" style={{ fontFamily: font }}>{font}</span>
                    </button>
                    <button
                      type="button"
                      aria-label={`${favorite ? unfavoriteFontLabel : favoriteFontLabel}: ${font}`}
                      aria-pressed={favorite}
                      onClick={() => toggleFavorite(font)}
                      className={`rounded p-1 hover:bg-bb-surface-3 ${favorite ? 'text-bb-accent' : 'text-bb-text-dim opacity-0 group-hover:opacity-100 focus:opacity-100'}`}
                    >
                      <Star size={13} fill={favorite ? STAR_FILLED : STAR_EMPTY} />
                    </button>
                  </div>
                </div>
              );
            })}
            {filteredFonts.length === 0 ? (
              <div className="px-3 py-6 text-center text-xs text-bb-text-dim">{noFontsFoundLabel}</div>
            ) : null}
          </div>
        </div>,
        document.body,
      )}
    </label>
  );
}
