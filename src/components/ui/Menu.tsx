"use client";

import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";

import { cx } from "@/lib/util";

export interface MenuItem {
  label: string;
  icon?: ReactNode;
  onSelect?: () => void;
  danger?: boolean;
  disabled?: boolean;
  hint?: string;
  separator?: boolean;
}

/** A floating menu anchored to a point. Closes on outside click / Escape. */
export function FloatingMenu({
  x,
  y,
  items,
  onClose,
  minWidth = 200,
}: {
  x: number;
  y: number;
  items: MenuItem[];
  onClose: () => void;
  minWidth?: number;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ x, y });

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const rect = el.getBoundingClientRect();
    const nx = Math.min(x, window.innerWidth - rect.width - 8);
    const ny = Math.min(y, window.innerHeight - rect.height - 8);
    if (nx !== pos.x || ny !== pos.y) setPos({ x: Math.max(8, nx), y: Math.max(8, ny) });
  }, [x, y, pos.x, pos.y]);

  useEffect(() => {
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey);
    window.addEventListener("blur", onClose);
    return () => {
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("blur", onClose);
    };
  }, [onClose]);

  return createPortal(
    <div
      ref={ref}
      role="menu"
      className="fixed z-50 rounded-[10px] border border-border-strong bg-elev p-1.5 shadow-[var(--shadow)]"
      style={{ left: pos.x, top: pos.y, minWidth }}
    >
      {items.map((item, i) =>
        item.separator ? (
          <div key={i} className="mx-1 my-1.5 h-px bg-border" />
        ) : item.disabled && !item.onSelect ? (
          <div key={i} className="px-2.5 pt-1.5 pb-1 text-[11px] font-semibold tracking-[0.06em] text-faint">
            {item.label}
          </div>
        ) : (
          <button
            key={i}
            role="menuitem"
            disabled={item.disabled}
            onClick={() => {
              onClose();
              item.onSelect?.();
            }}
            className={cx(
              "flex h-8 w-full items-center gap-2 rounded-[7px] px-2.5 text-left text-[13px] disabled:opacity-40",
              item.danger ? "text-danger hover:bg-danger-soft" : "text-text-2 hover:bg-hover hover:text-text",
            )}
          >
            <span className="flex w-4 justify-center text-muted">{item.icon}</span>
            <span className="flex-1 truncate">{item.label}</span>
            {item.hint && <span className="text-[11.5px] text-faint">{item.hint}</span>}
          </button>
        ),
      )}
    </div>,
    document.body,
  );
}

/** Hook for context menus: returns props to spread + the rendered menu. */
export function useContextMenu(items: () => MenuItem[]) {
  const [at, setAt] = useState<{ x: number; y: number } | null>(null);
  const open = (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setAt({ x: e.clientX, y: e.clientY });
  };
  const menu = at ? <FloatingMenu x={at.x} y={at.y} items={items()} onClose={() => setAt(null)} /> : null;
  return { open, menu, isOpen: at !== null };
}

/** A button that opens a dropdown menu below it. */
export function MenuButton({
  items,
  children,
  className,
  title,
  minWidth,
}: {
  items: MenuItem[] | (() => MenuItem[]);
  children: ReactNode;
  className?: string;
  title?: string;
  minWidth?: number;
}) {
  const [at, setAt] = useState<{ x: number; y: number } | null>(null);
  return (
    <>
      <button
        type="button"
        title={title}
        aria-haspopup="menu"
        className={className}
        onClick={(e) => {
          e.stopPropagation();
          const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
          setAt(at ? null : { x: rect.left, y: rect.bottom + 4 });
        }}
      >
        {children}
      </button>
      {at && (
        <FloatingMenu
          x={at.x}
          y={at.y}
          minWidth={minWidth}
          items={typeof items === "function" ? items() : items}
          onClose={() => setAt(null)}
        />
      )}
    </>
  );
}
