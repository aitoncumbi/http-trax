"use client";

import { forwardRef, useCallback, useImperativeHandle, useRef } from "react";

import { cx } from "@/lib/util";

/** Split text into plain runs and `{{variable}}` tokens. */
function tokens(value: string): { text: string; isVar: boolean }[] {
  return value
    .split(/(\{\{[^{}]*\}\})/g)
    .filter((t) => t !== "")
    .map((text) => ({ text, isVar: /^\{\{[^{}]*\}\}$/.test(text) }));
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
 */
export const VarInput = forwardRef<HTMLInputElement, Props>(function VarInput({ value, onChange, className, ...rest }, ref) {
  const input = useRef<HTMLInputElement>(null);
  const layer = useRef<HTMLDivElement>(null);
  useImperativeHandle(ref, () => input.current as HTMLInputElement);

  const sync = useCallback(() => {
    if (input.current && layer.current) layer.current.scrollLeft = input.current.scrollLeft;
  }, []);

  return (
    <div className={cx("var-input", className)}>
      <div ref={layer} className="var-layer" aria-hidden>
        <span className="whitespace-pre">
          {tokens(value).map((t, i) =>
            t.isVar ? (
              <span key={i} className="var-token">
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
        onChange={(e) => {
          onChange(e.target.value);
          requestAnimationFrame(sync);
        }}
        onScroll={sync}
        onKeyUp={sync}
        onSelect={sync}
        onFocus={sync}
        {...rest}
      />
    </div>
  );
});
