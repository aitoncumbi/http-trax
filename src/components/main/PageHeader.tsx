"use client";

import { Save } from "lucide-react";
import type { ReactNode } from "react";

import { cx } from "@/lib/util";

/** Title row for editors that are not requests: collection, folder, environment, run. */
export function PageHeader({
  eyebrow,
  title,
  onTitle,
  children,
}: {
  eyebrow: ReactNode;
  title: string;
  /** Makes the title editable. */
  onTitle?: (v: string) => void;
  children?: ReactNode;
}) {
  return (
    <div className="flex shrink-0 items-end gap-2.5 px-5 pt-4 pb-3.5">
      <div className="min-w-0 flex-1">
        <div className="eyebrow mb-1">{eyebrow}</div>
        {onTitle ? (
          <input
            aria-label="Name"
            value={title}
            onChange={(e) => onTitle(e.target.value)}
            className="-mx-1 w-full rounded-md bg-transparent px-1 text-[20px] font-semibold tracking-[-0.015em] outline-none hover:bg-hover focus:bg-panel-2"
          />
        ) : (
          <div className="truncate text-[20px] font-semibold tracking-[-0.015em]">{title}</div>
        )}
      </div>
      {children}
    </div>
  );
}

export function SaveButton({ dirty, onSave }: { dirty: boolean; onSave: () => void }) {
  return (
    <button
      type="button"
      onClick={onSave}
      disabled={!dirty}
      title="Save (Ctrl+S)"
      className={cx(
        "inline-flex h-9 items-center gap-[9px] rounded-lg border px-3.5 text-[13.5px] font-medium",
        dirty ? "border-accent-line text-text hover:bg-hover" : "border-border-strong text-muted",
      )}
    >
      <Save size={15} strokeWidth={1.4} />
      Save
    </button>
  );
}
