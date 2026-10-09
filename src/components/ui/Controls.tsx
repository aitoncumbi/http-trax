"use client";

import { Check } from "lucide-react";

import { cx } from "@/lib/util";

export function Checkbox({
  checked,
  onChange,
  label,
  size = 18,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
  size?: number;
}) {
  return (
    <button
      type="button"
      role="checkbox"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange(!checked)}
      className={cx(
        "inline-flex shrink-0 items-center justify-center rounded-[5px] border-[1.5px] transition-colors",
        checked ? "border-accent bg-accent" : "border-checkbox-off bg-transparent hover:border-muted",
      )}
      style={{ width: size, height: size }}
    >
      {checked && <Check size={size - 7} strokeWidth={3.2} className="text-white" />}
    </button>
  );
}

export function Switch({ checked, onChange, label }: { checked: boolean; onChange: (checked: boolean) => void; label: string }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange(!checked)}
      className={cx("relative h-[22px] w-[38px] shrink-0 rounded-full transition-colors", checked ? "bg-accent" : "bg-border-strong")}
    >
      <span
        className="absolute top-[2px] h-[18px] w-[18px] rounded-full bg-white shadow-sm transition-[left]"
        style={{ left: checked ? 18 : 2 }}
      />
    </button>
  );
}

export function Radio({ checked, onChange, children }: { checked: boolean; onChange: () => void; children: React.ReactNode }) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={checked}
      onClick={onChange}
      className={cx("inline-flex items-center gap-2 text-[13.5px] whitespace-nowrap", checked ? "text-text" : "text-muted hover:text-text-2")}
    >
      <span
        className={cx(
          "inline-flex h-[15px] w-[15px] items-center justify-center rounded-full border-[1.5px]",
          checked ? "border-accent-line" : "border-checkbox-off",
        )}
      >
        {checked && <span className="h-[7px] w-[7px] rounded-full bg-accent-line" />}
      </span>
      {children}
    </button>
  );
}
