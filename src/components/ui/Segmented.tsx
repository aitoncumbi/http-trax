"use client";

import type { ReactNode } from "react";

import { cx, scrollSideways } from "@/lib/util";

export interface SegmentedTab<T extends string> {
  id: T;
  label: ReactNode;
  badge?: ReactNode;
  /** Badge colour: neutral by default. */
  tone?: "ok" | "danger" | "accent";
}

const TONES = { ok: "text-ok", danger: "text-danger", accent: "text-accent-text" } as const;

/** Underlined tab strip (36px) used in editors and viewers. Labels start at the 16px content inset. */
export function TabStrip<T extends string>({
  tabs,
  value,
  onChange,
  right,
  className,
  label,
}: {
  tabs: SegmentedTab<T>[];
  value: T;
  onChange: (v: T) => void;
  right?: ReactNode;
  className?: string;
  label?: string;
}) {
  return (
    <div className={cx("tab-strip flex h-9 shrink-0 items-stretch justify-between gap-3", className)}>
      <div role="tablist" aria-label={label} onWheel={scrollSideways} className="tab-scroll flex min-w-0 items-stretch px-1">
        {tabs.map((t) => (
          <button
            key={t.id}
            type="button"
            role="tab"
            aria-selected={value === t.id}
            onClick={() => onChange(t.id)}
            className={cx(
              "inline-flex items-center gap-1.5 border-b-2 px-3 text-[13px] whitespace-nowrap transition-colors",
              value === t.id ? "border-accent-line font-medium text-text" : "border-transparent text-muted hover:text-text",
            )}
          >
            {t.label}
            {t.badge !== undefined && t.badge !== null && t.badge !== "" && t.badge !== 0 && (
              <span className={cx("text-[11.5px] font-normal", t.tone ? TONES[t.tone] : "text-faint")}>{t.badge}</span>
            )}
          </button>
        ))}
      </div>
      {right && <div className="flex shrink-0 items-center gap-1 pr-4">{right}</div>}
    </div>
  );
}

/** Pill toggle group (Pretty / Raw). */
export function Pills<T extends string>({
  options,
  value,
  onChange,
}: {
  options: { id: T; label: string }[];
  value: T;
  onChange: (v: T) => void;
}) {
  return (
    <div className="segmented">
      {options.map((o) => (
        <button key={o.id} type="button" aria-pressed={value === o.id} onClick={() => onChange(o.id)}>
          {o.label}
        </button>
      ))}
    </div>
  );
}
