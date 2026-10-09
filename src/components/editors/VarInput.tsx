"use client";

import { forwardRef, useCallback, useId, useImperativeHandle, useRef, useState } from "react";

import { cx } from "@/lib/util";
import { completeVar, openVarAt, suggest, type VarInfo } from "@/lib/variables";

import { FloatingCard, useVarScope, VarCard, VarSuggestions } from "./VarScope";

/** Split text into plain runs and `{{variable}}` tokens. */
function tokens(value: string): { text: string; name: string | null }[] {
  return value
    .split(/(\{\{[^{}]*\}\})/g)
    .filter((t) => t !== "")
    .map((text) => {
      const m = /^\{\{([^{}]*)\}\}$/.exec(text);
      return { text, name: m ? m[1].trim() : null };
    });
}

interface Props extends Omit<React.InputHTMLAttributes<HTMLInputElement>, "value" | "onChange"> {
  value: string;
  onChange: (value: string) => void;
  /** Classes for the wrapper (font, height, colour). */
  className?: string;
}

/**
 * A single-line input that highlights `{{variables}}`. The real input sits on
 * top with transparent text; a mirror layer underneath renders the colours.
 * Highlights use background + box-shadow only, so glyph positions line up.
 *
 * Inside a variable scope it also completes `{{` and shows what a variable
 * resolves to on hover.
 */
export const VarInput = forwardRef<HTMLInputElement, Props>(function VarInput({ value, onChange, className, onKeyDown, onBlur, ...rest }, ref) {
  const input = useRef<HTMLInputElement>(null);
  const layer = useRef<HTMLDivElement>(null);
  const scope = useVarScope();
  const [menu, setMenu] = useState<{ items: VarInfo[]; active: number; rect: DOMRect } | null>(null);
  const [hover, setHover] = useState<{ name: string; rect: DOMRect } | null>(null);
  const listId = useId();
  useImperativeHandle(ref, () => input.current as HTMLInputElement);

  const sync = useCallback(() => {
    if (input.current && layer.current) layer.current.scrollLeft = input.current.scrollLeft;
  }, []);

  const refreshMenu = (text: string) => {
    const el = input.current;
    if (!scope || !el) return;
    const open = openVarAt(text, el.selectionStart ?? text.length);
    const items = open ? suggest(scope, open.query) : [];
    setMenu(items.length ? { items, active: 0, rect: el.getBoundingClientRect() } : null);
  };

  const pick = (v: VarInfo) => {
    const el = input.current;
    if (!el) return;
    const done = completeVar(value, el.selectionStart ?? value.length, v.name);
    setMenu(null);
    if (!done) return;
    onChange(done.text);
    requestAnimationFrame(() => {
      el.setSelectionRange(done.caret, done.caret);
      sync();
    });
  };

  const onMouseMove = (e: React.MouseEvent) => {
    if (!scope || !layer.current) return;
    for (const el of layer.current.querySelectorAll<HTMLElement>("[data-var]")) {
      const r = el.getBoundingClientRect();
      if (e.clientX >= r.left && e.clientX <= r.right && e.clientY >= r.top && e.clientY <= r.bottom) {
        if (hover?.name !== el.dataset.var) setHover({ name: el.dataset.var ?? "", rect: r });
        return;
      }
    }
    if (hover) setHover(null);
  };

  return (
    <div className={cx("var-input", className)} onMouseMove={onMouseMove} onMouseLeave={() => setHover(null)}>
      <div ref={layer} className="var-layer" aria-hidden>
        <span className="whitespace-pre">
          {tokens(value).map((t, i) =>
            t.name !== null ? (
              <span key={i} data-var={t.name} className={scope && !scope.has(t.name) ? "var-token-missing" : "var-token"}>
                {t.text}
              </span>
            ) : (
              <span key={i}>{t.text}</span>
            ),
          )}
          {/* keeps the layer as wide as the input's scroll width */}
          <span className="inline-block w-6" />
        </span>
      </div>
      <input
        ref={input}
        value={value}
        spellCheck={false}
        autoComplete="off"
        role={scope ? "combobox" : undefined}
        aria-autocomplete={scope ? "list" : undefined}
        aria-expanded={scope ? !!menu : undefined}
        aria-controls={menu ? listId : undefined}
        onChange={(e) => {
          onChange(e.target.value);
          refreshMenu(e.target.value);
          requestAnimationFrame(sync);
        }}
        onScroll={sync}
        onKeyUp={(e) => {
          sync();
          if (e.key === "ArrowLeft" || e.key === "ArrowRight" || e.key === "Home" || e.key === "End") refreshMenu(value);
        }}
        onSelect={sync}
        onFocus={sync}
        onBlur={(e) => {
          setMenu(null);
          onBlur?.(e);
        }}
        onKeyDown={(e) => {
          if (menu) {
            if (e.key === "ArrowDown" || e.key === "ArrowUp") {
              e.preventDefault();
              const n = menu.items.length;
              setMenu({ ...menu, active: (menu.active + (e.key === "ArrowDown" ? 1 : n - 1)) % n });
              return;
            }
            if (e.key === "Enter" || e.key === "Tab") {
              e.preventDefault();
              e.stopPropagation();
              pick(menu.items[menu.active]);
              return;
            }
            if (e.key === "Escape") {
              e.preventDefault();
              e.stopPropagation();
              setMenu(null);
              return;
            }
          }
          onKeyDown?.(e);
        }}
        {...rest}
      />
      {menu && <VarSuggestions id={listId} items={menu.items} active={menu.active} rect={menu.rect} onPick={pick} onHover={(i) => setMenu({ ...menu, active: i })} />}
      {hover && !menu && scope && (
        <FloatingCard rect={hover.rect}>
          <VarCard name={hover.name} info={scope.get(hover.name)} />
        </FloatingCard>
      )}
    </div>
  );
});
