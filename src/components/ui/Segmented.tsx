"use client";

import type { ReactNode } from "react";

import { cx } from "@/lib/util";

export interface SegmentedTab<T extends string> {
  id: T;
  label: ReactNode;
  badge?: ReactNode;
}

/** Underlined tab strip used inside editors and viewers. */
export function TabStrip<T extends string>({
  tabs,
  value,
  onChange,
  right,
  className,
}: {
  tabs: SegmentedTab<T>[];
  value: T;
  onChange: (v: T) => void;
  right?: ReactNode;
  className?: string;
}) {
  return (
    <div className={cx("flex items-center gap-1 border-b border-border px-3", className)} role="tablist">
      {tabs.map((t) => (
        <button
          key={t.id}
          role="tab"
          aria-selected={value === t.id}
          onClick={() => onChange(t.id)}
          className={cx(
            "relative -mb-px flex h-9 items-center gap-1.5 border-b-2 px-2 text-[12.5px] transition-colors",
            value === t.id ? "border-accent text-text" : "border-transparent text-muted hover:text-text",
          )}
        >
          {t.label}
          {t.badge !== undefined && t.badge !== null && t.badge !== 0 && (
            <span className="rounded bg-hover px-1.5 text-[10.5px] font-semibold text-muted">{t.badge}</span>
          )}
        </button>
      ))}
      <div className="ml-auto flex items-center gap-1">{right}</div>
    </div>
  );
}

/** Pill-style toggle group (e.g. Pretty / Raw). */
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
    <div className="inline-flex rounded-md border border-border bg-panel-2 p-0.5">
      {options.map((o) => (
        <button
          key={o.id}
          onClick={() => onChange(o.id)}
          className={cx(
            "h-6 rounded px-2 text-[11.5px] font-medium",
            value === o.id ? "bg-elev text-text shadow-sm" : "text-muted hover:text-text",
          )}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}
