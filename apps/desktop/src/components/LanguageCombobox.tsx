import {
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
} from "react";
import { createPortal } from "react-dom";
import { Check, ChevronDown, Globe, Search, X } from "lucide-react";
import { flag, languages, type LanguageOption } from "../data/languages";

interface LanguageComboboxProps {
  value: string;
  onChange: (code: string) => void;
  /** Restrict the option list to these ISO codes. `auto` is always shown.
   *  When omitted, all 99 languages are available. */
  supportedCodes?: readonly string[];
}

const AUTO: LanguageOption = { code: "auto", name: "Auto-detect", country: "" };
const MENU_WIDTH = 300;
const MENU_MARGIN = 8;

export function LanguageCombobox({
  value,
  onChange,
  supportedCodes,
}: LanguageComboboxProps) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [highlight, setHighlight] = useState(0);
  const [menuStyle, setMenuStyle] = useState<CSSProperties>({});
  const triggerRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  const options = useMemo<LanguageOption[]>(() => {
    const filter = supportedCodes ? new Set(supportedCodes) : null;
    const list = filter
      ? languages.filter((l) => filter.has(l.code))
      : languages;
    const all = [AUTO, ...list];
    if (!query.trim()) return all;
    const q = query.trim().toLowerCase();
    return all.filter(
      (item) =>
        item.name.toLowerCase().includes(q) ||
        item.code.toLowerCase().startsWith(q),
    );
  }, [query, supportedCodes]);

  const selected =
    value === "auto" ? AUTO : languages.find((l) => l.code === value) ?? AUTO;

  useEffect(() => {
    if (!open) return;
    setQuery("");
    setHighlight(0);
    const raf = window.setTimeout(() => inputRef.current?.focus(), 30);
    return () => window.clearTimeout(raf);
  }, [open]);

  // Close on outside interaction / scroll / resize.
  useEffect(() => {
    if (!open) return;
    function onDown(event: MouseEvent) {
      const target = event.target as Node;
      if (
        !triggerRef.current?.contains(target) &&
        !menuRef.current?.contains(target)
      ) {
        setOpen(false);
      }
    }
    function onEsc(event: globalThis.KeyboardEvent) {
      if (event.key === "Escape") setOpen(false);
    }
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onEsc);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onEsc);
    };
  }, [open]);

  // Position the menu in viewport coordinates so backdrop-filter'd ancestors
  // don't trap it. Aligns the menu's right edge with the trigger's right edge,
  // flips above when there isn't enough room below.
  useLayoutEffect(() => {
    if (!open) return;
    const updatePosition = () => {
      const rect = triggerRef.current?.getBoundingClientRect();
      if (!rect) return;
      const menuHeight = menuRef.current?.offsetHeight ?? 360;
      const spaceBelow = window.innerHeight - rect.bottom - MENU_MARGIN;
      const openUp = spaceBelow < menuHeight && rect.top > menuHeight + MENU_MARGIN;
      const top = openUp ? rect.top - menuHeight - 6 : rect.bottom + 6;
      const left = Math.max(
        MENU_MARGIN,
        Math.min(
          window.innerWidth - MENU_WIDTH - MENU_MARGIN,
          rect.right - MENU_WIDTH,
        ),
      );
      setMenuStyle({
        position: "fixed",
        top,
        left,
        width: MENU_WIDTH,
        zIndex: 1000,
      });
    };
    updatePosition();
    window.addEventListener("resize", updatePosition);
    window.addEventListener("scroll", updatePosition, true);
    return () => {
      window.removeEventListener("resize", updatePosition);
      window.removeEventListener("scroll", updatePosition, true);
    };
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const el = listRef.current?.querySelector<HTMLElement>(
      `[data-index="${highlight}"]`,
    );
    el?.scrollIntoView({ block: "nearest" });
  }, [highlight, open]);

  function select(code: string) {
    onChange(code);
    setOpen(false);
  }

  function onKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setHighlight((h) => Math.min(options.length - 1, h + 1));
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setHighlight((h) => Math.max(0, h - 1));
    } else if (event.key === "Enter") {
      event.preventDefault();
      const pick = options[highlight];
      if (pick) select(pick.code);
    }
  }

  const menu = open ? (
    <div
      className="combobox__menu"
      ref={menuRef}
      role="listbox"
      style={menuStyle}
    >
      <div className="combobox__search">
        <Search size={13} strokeWidth={1.8} />
        <input
          ref={inputRef}
          type="text"
          value={query}
          placeholder={`Search ${supportedCodes ? supportedCodes.length : languages.length} languages…`}
          onChange={(event) => {
            setQuery(event.target.value);
            setHighlight(0);
          }}
          onKeyDown={onKeyDown}
        />
        {query ? (
          <button
            type="button"
            className="combobox__clear"
            onClick={() => {
              setQuery("");
              inputRef.current?.focus();
            }}
            aria-label="Clear search"
          >
            <X size={12} strokeWidth={2} />
          </button>
        ) : null}
      </div>
      <div className="combobox__list" ref={listRef}>
        {options.length === 0 ? (
          <div className="combobox__empty">No matches</div>
        ) : (
          options.map((opt, i) => (
            <button
              type="button"
              key={opt.code}
              data-index={i}
              role="option"
              aria-selected={opt.code === value}
              className={`combobox__option${i === highlight ? " is-highlight" : ""}${opt.code === value ? " is-selected" : ""}`}
              onMouseEnter={() => setHighlight(i)}
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => select(opt.code)}
            >
              <span className="combobox__option-flag" aria-hidden="true">
                {opt.country ? (
                  <span className="combobox__flag">{flag(opt.country)}</span>
                ) : (
                  <Globe size={15} strokeWidth={1.8} />
                )}
              </span>
              <span className="combobox__name">{opt.name}</span>
              {opt.code === value ? (
                <Check size={13} strokeWidth={2.4} className="combobox__check" />
              ) : null}
            </button>
          ))
        )}
      </div>
    </div>
  ) : null;

  return (
    <div className={`combobox${open ? " is-open" : ""}`}>
      <button
        type="button"
        ref={triggerRef}
        className="combobox__trigger"
        onClick={() => setOpen((s) => !s)}
        aria-haspopup="listbox"
        aria-expanded={open}
      >
        <span className="combobox__trigger-flag" aria-hidden="true">
          {selected.country ? (
            <span className="combobox__flag">{flag(selected.country)}</span>
          ) : (
            <Globe size={14} strokeWidth={1.8} />
          )}
        </span>
        <span className="combobox__trigger-text">{selected.name}</span>
        <ChevronDown size={13} strokeWidth={2} className="combobox__caret" />
      </button>
      {menu && typeof document !== "undefined"
        ? createPortal(menu, document.body)
        : null}
    </div>
  );
}
