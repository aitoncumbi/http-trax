"use client";

import { CheckCircle2, Info, X, XCircle } from "lucide-react";

import { useStore } from "@/lib/store";
import { cx } from "@/lib/util";

export function Toasts() {
  const toasts = useStore((s) => s.toasts);
  const dismiss = useStore((s) => s.dismissToast);
  return (
    <div className="pointer-events-none fixed right-4 bottom-10 z-[60] flex w-[360px] flex-col gap-2" aria-live="polite">
      {toasts.map((t) => (
        <div
          key={t.id}
          className={cx(
            "pointer-events-auto flex items-start gap-2.5 rounded-lg border bg-elev px-3 py-2.5 text-[13px] shadow-[var(--shadow)]",
            t.kind === "error" ? "border-danger/40" : "border-border-strong",
          )}
        >
          <span className="mt-0.5">
            {t.kind === "error" ? (
              <XCircle size={16} className="text-danger" />
            ) : t.kind === "success" ? (
              <CheckCircle2 size={16} className="text-ok" />
            ) : (
              <Info size={16} className="text-info" />
            )}
          </span>
          <p className="flex-1 break-words whitespace-pre-wrap">{t.message}</p>
          <button className="text-faint hover:text-text" onClick={() => dismiss(t.id)} aria-label="Dismiss">
            <X size={14} />
          </button>
        </div>
      ))}
    </div>
  );
}
