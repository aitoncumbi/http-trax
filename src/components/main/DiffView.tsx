"use client";

import { cx } from "@/lib/util";

/** Renders unified diff / `git show` output with add/remove colouring. */
export function DiffView({ diff, className }: { diff: string; className?: string }) {
  const lines = diff.split("\n");
  return (
    <div className={cx("min-h-0 flex-1 overflow-auto font-mono text-[12px] leading-[1.55]", className)}>
      {lines.map((line, i) => {
        const file = line.startsWith("diff --git");
        const meta = line.startsWith("index ") || line.startsWith("--- ") || line.startsWith("+++ ") || line.startsWith("new file") || line.startsWith("deleted file") || line.startsWith("similarity") || line.startsWith("rename ");
        const hunk = line.startsWith("@@");
        const add = !meta && line.startsWith("+");
        const del = !meta && line.startsWith("-");
        const conflict = line.startsWith("<<<<<<<") || line.startsWith("=======") || line.startsWith(">>>>>>>") || line.startsWith("++<<<<<<<") || line.startsWith("++=======") || line.startsWith("++>>>>>>>");
        return (
          <div
            key={i}
            className={cx(
              "px-4 whitespace-pre-wrap break-all",
              file && "mt-3 border-y border-border bg-panel-2 py-1 font-semibold text-text first:mt-0",
              meta && !file && "text-faint",
              hunk && "bg-info/10 text-info",
              add && "bg-[var(--diff-add)] text-text",
              del && "bg-[var(--diff-del)] text-text",
              conflict && "bg-warn/15 font-semibold text-warn",
              !file && !meta && !hunk && !add && !del && !conflict && "text-muted",
            )}
          >
            {line || " "}
          </div>
        );
      })}
    </div>
  );
}
