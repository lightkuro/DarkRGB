// Right-click menus. Anything can offer one: it calls `openMenu` from its
// onContextMenu handler with what to show, and the one host mounted in
// the app draws it where the pointer is, kept inside the window.

import { useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore, type ReactNode } from "react";
import { Store } from "../engine";

export interface MenuItem {
  label: string;
  icon?: ReactNode;
  action: () => void;
  danger?: boolean;
  disabled?: boolean;
}

/** A line between groups of items. */
export const SEPARATOR = "-" as const;

interface OpenMenu {
  x: number;
  y: number;
  title?: string;
  items: (MenuItem | typeof SEPARATOR)[];
}

const menu = new Store<OpenMenu | null>(null);

/**
 * Opens a menu where the pointer is. Pass the event of an onContextMenu
 * handler; the browser's own menu is kept away. A keyboard user gets the
 * same menu from the context-menu key or Shift+F10, which also send it.
 */
export function openMenu(
  event: { preventDefault: () => void; stopPropagation: () => void; clientX: number; clientY: number; currentTarget: EventTarget },
  items: (MenuItem | typeof SEPARATOR)[],
  title?: string,
): void {
  event.preventDefault();
  event.stopPropagation();
  let { clientX: x, clientY: y } = event;
  // From the keyboard there is no pointer: open by the thing itself.
  if (x === 0 && y === 0 && event.currentTarget instanceof Element) {
    const rect = event.currentTarget.getBoundingClientRect();
    x = rect.left + 12;
    y = rect.bottom - 4;
  }
  menu.set({ x, y, title, items });
}

export function closeMenu(): void {
  menu.set(null);
}

export function ContextMenuHost() {
  const open = useSyncExternalStore(menu.subscribe, menu.get);
  const ref = useRef<HTMLDivElement>(null);
  const [place, setPlace] = useState<{ left: number; top: number } | null>(null);

  // Kept inside the window: moved left or up if it would run off.
  useLayoutEffect(() => {
    if (!open || !ref.current) {
      setPlace(null);
      return;
    }
    const { width, height } = ref.current.getBoundingClientRect();
    setPlace({
      left: Math.max(4, Math.min(open.x, window.innerWidth - width - 4)),
      top: Math.max(4, Math.min(open.y, window.innerHeight - height - 4)),
    });
    ref.current.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const away = (e: Event) => {
      if (ref.current && e.target instanceof Node && ref.current.contains(e.target)) return;
      closeMenu();
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        closeMenu();
        return;
      }
      if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
      e.preventDefault();
      const buttons = [...(ref.current?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ?? [])];
      const at = buttons.indexOf(document.activeElement as HTMLButtonElement);
      const next = (at + (e.key === "ArrowDown" ? 1 : -1) + buttons.length) % buttons.length;
      buttons[next]?.focus();
    };
    window.addEventListener("pointerdown", away, true);
    window.addEventListener("keydown", key, true);
    window.addEventListener("blur", closeMenu);
    window.addEventListener("resize", closeMenu);
    window.addEventListener("wheel", closeMenu, { passive: true });
    return () => {
      window.removeEventListener("pointerdown", away, true);
      window.removeEventListener("keydown", key, true);
      window.removeEventListener("blur", closeMenu);
      window.removeEventListener("resize", closeMenu);
      window.removeEventListener("wheel", closeMenu);
    };
  }, [open]);

  if (!open) return null;
  return (
    <div
      ref={ref}
      className="context-menu"
      role="menu"
      aria-label={open.title ?? "Actions"}
      style={{ left: place?.left ?? open.x, top: place?.top ?? open.y, visibility: place ? "visible" : "hidden" }}
      onContextMenu={(e) => e.preventDefault()}
    >
      {open.title && <div className="context-title">{open.title}</div>}
      {open.items.map((item, i) =>
        item === SEPARATOR ? (
          <div key={i} className="context-separator" role="separator" />
        ) : (
          <button
            key={i}
            type="button"
            role="menuitem"
            className={item.danger ? "danger" : ""}
            disabled={item.disabled}
            onClick={() => {
              closeMenu();
              item.action();
            }}
          >
            <span className="context-icon" aria-hidden>
              {item.icon}
            </span>
            {item.label}
          </button>
        ),
      )}
    </div>
  );
}
